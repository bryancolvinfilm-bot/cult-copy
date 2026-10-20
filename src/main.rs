#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

use anyhow::{Context, Result};
use cult_copy::{
    config::{self, Config},
    engine, ui,
};
use std::{fs::OpenOptions, path::PathBuf};

fn main() {
    if let Err(error) = run() {
        eprintln!("Cult Copy: {error:#}");
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
        println!("Cult Copy {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    if args.iter().any(|a| a == "--help") {
        println!("Cult Copy\n\n  --background       Start in the tray/menu bar\n  --data-dir PATH    Use a separate settings/cache folder\n  --version          Print the app version\n");
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
        cult_copy::instance::activate(&dir)
            .context("Cult Copy is already running; open it from the tray/menu bar")?;
        return Ok(());
    }
    let log = OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("app.log"))?;
    tracing_subscriber::fmt()
        .with_ansi(false)
        .with_env_filter("cult_copy=info,warn")
        .with_writer(std::sync::Mutex::new(log))
        .init();
    let config = Config::load(&dir)?;
    // Refresh the existing startup entry after a bundle move or AppImage update.
    // Keep the legacy startup IDs so this replaces the entry instead of adding one.
    if config.start_at_login {
        if let Err(error) = config::set_autostart(true) {
            tracing::warn!("Could not refresh start at login: {error:#}");
        }
    }
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    let handle = engine::start(config, dir.clone(), &runtime);
    cult_copy::instance::listen(&dir, handle.clone(), &runtime)?;
    let shutdown = handle.clone();
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Cult Copy")
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
    #[cfg(target_os = "linux")]
    let window_result = cult_copy::window::run_linux(
        handle,
        options,
        args.iter().any(|arg| arg == "--background"),
    );
    #[cfg(not(target_os = "linux"))]
    let window_result = eframe::run_native(
        "Cult Copy",
        options,
        Box::new(move |cc| Ok(Box::new(ui::App::new(cc, handle)))),
    )
    .map_err(|error| anyhow::anyhow!("Cannot open app window: {error}"));
    shutdown.shutdown();
    // Dropping a runtime waits indefinitely for blocking file I/O. Give active
    // work a bounded cleanup window so tray Quit always lets the app restart.
    runtime.shutdown_timeout(std::time::Duration::from_secs(2));
    drop(lock);
    window_result
}
