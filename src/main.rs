#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

use anyhow::{Context, Result};
use duet_clipboard::{
    config::{self, Config},
    engine, ui,
};
use std::{fs::OpenOptions, path::PathBuf};

fn main() {
    if let Err(error) = run() {
        eprintln!("Duet Clipboard: {error:#}");
        if let Ok(dir) = config::default_dir() {
            let _ = std::fs::create_dir_all(&dir);
            let _ = std::fs::write(dir.join("startup-error.txt"), format!("{error:#}"));
        }
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    if args.iter().any(|a| a == "--version") {
        println!("Duet Clipboard {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    if args.iter().any(|a| a == "--help") {
        println!("Duet Clipboard\n\n  --background       Start in the tray/menu bar\n  --data-dir PATH    Use a separate settings/cache folder\n  --version          Print the app version\n");
        return Ok(());
    }
    let dir = match args.iter().position(|a| a == "--data-dir") {
        Some(index) => PathBuf::from(
            args.get(index + 1)
                .context("--data-dir requires a folder")?,
        ),
        None => config::default_dir()?,
    };
    std::fs::create_dir_all(&dir)?;
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(dir.join("instance.lock"))?;
    if fs2::FileExt::try_lock_exclusive(&lock).is_err() {
        duet_clipboard::instance::activate(&dir)
            .context("Duet Clipboard is already running; open it from the tray/menu bar")?;
        return Ok(());
    }
    let log = OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("app.log"))?;
    tracing_subscriber::fmt()
        .with_ansi(false)
        .with_env_filter("duet_clipboard=info,warn")
        .with_writer(std::sync::Mutex::new(log))
        .init();
    let config = Config::load(&dir)?;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    let handle = engine::start(config, dir.clone(), &runtime);
    duet_clipboard::instance::listen(&dir, handle.clone(), &runtime)?;
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Duet Clipboard")
            .with_app_id("duet-clipboard")
            .with_inner_size([560.0, 720.0])
            .with_min_inner_size([440.0, 420.0])
            .with_visible(!args.iter().any(|a| a == "--background"))
            .with_icon(eframe::egui::IconData {
                rgba: ui::icon_rgba(),
                width: 32,
                height: 32,
            }),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    eframe::run_native(
        "Duet Clipboard",
        options,
        Box::new(move |cc| Ok(Box::new(ui::App::new(cc, handle)))),
    )
    .map_err(|error| anyhow::anyhow!("Cannot open app window: {error}"))?;
    drop(runtime);
    drop(lock);
    Ok(())
}
