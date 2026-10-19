//! The tray and clipboard engine outlive the optional native window. Dropping
//! the surface works on Wayland as well as X11; eframe reuses its event loop
//! when Open creates the next window on the same main thread.
use super::{Action, WindowControl};
use crate::{
    engine::Handle,
    ui::{App, Tray},
};
use anyhow::{Context, Result};
use eframe::egui::{self, ViewportCommand};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc, Mutex,
};

#[derive(Default)]
pub(super) struct State {
    context: Option<egui::Context>,
    open_requested: bool,
    #[cfg(feature = "ui-test")]
    opened: u64,
    #[cfg(feature = "ui-test")]
    rendered: bool,
    #[cfg(feature = "ui-test")]
    backend: &'static str,
}

struct Resident {
    control: WindowControl,
    quitting: Arc<AtomicBool>,
    attention: Arc<AtomicBool>,
    wake: mpsc::Receiver<()>,
}

impl Resident {
    fn new(background: bool) -> Self {
        let state = Arc::new(Mutex::new(State {
            open_requested: !background,
            ..Default::default()
        }));
        let quitting = Arc::new(AtomicBool::new(false));
        let attention = Arc::new(AtomicBool::new(false));
        let (sender, actions) = mpsc::channel();
        // Coalesce model changes while the main thread is drawing a window.
        let (wake, receiver) = mpsc::sync_channel(1);
        let control = WindowControl {
            sender,
            state: state.clone(),
        };
        let quit = quitting.clone();
        let show_attention = attention.clone();
        std::thread::Builder::new()
            .name("clipboard-window".into())
            .spawn(move || {
                while let Ok(action) = actions.recv() {
                    if matches!(action, Action::Stop) {
                        break;
                    }
                    if matches!(action, Action::Quit) {
                        quit.store(true, Ordering::Relaxed);
                    }
                    if matches!(action, Action::Refresh(true)) {
                        show_attention.store(true, Ordering::Relaxed);
                    }
                    let context = {
                        let mut state = state.lock().unwrap();
                        if matches!(action, Action::Open | Action::Refresh(true)) {
                            state.open_requested = state.context.is_none();
                        }
                        state.context.clone()
                    };
                    // Never hold the state lock while taking egui's context lock:
                    // the UI detaches its window during a close event.
                    if let Some(context) = &context {
                        match action {
                            Action::Open => {
                                context.send_viewport_cmd(ViewportCommand::Minimized(false));
                                context.send_viewport_cmd(ViewportCommand::Focus);
                            }
                            Action::Refresh(true) => {
                                context.send_viewport_cmd(ViewportCommand::RequestUserAttention(
                                    egui::UserAttentionType::Informational,
                                ))
                            }
                            Action::Quit => context.send_viewport_cmd(ViewportCommand::Close),
                            #[cfg(feature = "ui-test")]
                            Action::CloseWindow => {
                                context.send_viewport_cmd(ViewportCommand::Close)
                            }
                            _ => {}
                        }
                        context.request_repaint();
                    }
                    if context.is_none() || matches!(action, Action::Quit) {
                        let _ = wake.try_send(());
                    }
                }
            })
            .expect("Could not start the window dispatcher");
        Self {
            control,
            quitting,
            attention,
            wake: receiver,
        }
    }

    fn take_open_request(&self) -> bool {
        std::mem::take(&mut self.control.state.lock().unwrap().open_requested)
    }
}

impl Drop for Resident {
    fn drop(&mut self) {
        self.control.detach();
        self.control.stop();
    }
}

impl WindowControl {
    fn attach(&self, context: egui::Context) {
        let mut state = self.state.lock().unwrap();
        state.context = Some(context);
        // Requests received during window creation are fulfilled by this window.
        state.open_requested = false;
        #[cfg(feature = "ui-test")]
        {
            state.opened += 1;
            state.rendered = false;
        }
    }

    pub(crate) fn detach(&self) {
        self.state.lock().unwrap().context = None;
    }

    #[cfg(feature = "ui-test")]
    pub(crate) fn rendered(&self) {
        self.state.lock().unwrap().rendered = true;
    }
}

pub fn run(handle: Handle, mut options: eframe::NativeOptions, background: bool) -> Result<()> {
    let resident = Resident::new(background);
    let notifier = resident.control.clone();
    handle.set_notifier(Arc::new(move |show| notifier.refresh(show)));
    handle.set_notification_sender(crate::notifications::start());
    let tray = Tray::new(&handle, &resident.control).context("Cannot start the system tray")?;
    // Wayland ignores Visible(false). Start-at-login instead omits the window
    // entirely, and every opened window is a new, visible native surface.
    options.viewport.visible = Some(true);
    options.run_and_return = true;
    #[cfg(feature = "ui-test")]
    start_lifecycle_test(&resident, handle.clone(), tray.test.clone(), background);
    while !resident.quitting.load(Ordering::Relaxed) {
        tray.refresh(&handle);
        if resident.take_open_request() {
            let control = resident.control.clone();
            let quitting = resident.quitting.clone();
            let attention = resident.attention.clone();
            let handle = handle.clone();
            let tray = tray.clone();
            eframe::run_native(
                "Cult Copy",
                options.clone(),
                Box::new(move |cc| {
                    control.attach(cc.egui_ctx.clone());
                    #[cfg(feature = "ui-test")]
                    {
                        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
                        control.state.lock().unwrap().backend =
                            match cc.window_handle().map(|handle| handle.as_raw()) {
                                Ok(RawWindowHandle::Wayland(_)) => "wayland",
                                Ok(RawWindowHandle::Xlib(_) | RawWindowHandle::Xcb(_)) => "x11",
                                _ => "unknown",
                            };
                    }
                    Ok(Box::new(App::with_tray(
                        cc,
                        handle,
                        control,
                        quitting,
                        attention,
                        Some(tray),
                    )))
                }),
            )
            .map_err(|error| anyhow::anyhow!("Cannot open app window: {error}"))?;
            resident.control.detach();
        } else if resident.wake.recv().is_err() {
            break;
        }
    }
    Ok(())
}

#[cfg(feature = "ui-test")]
fn start_lifecycle_test(
    resident: &Resident,
    handle: Handle,
    tray: crate::ui::TrayTest,
    background: bool,
) {
    use anyhow::ensure;
    use std::time::{Duration, Instant};
    let Ok(report) = std::env::var("CULT_COPY_LIFECYCLE_TEST_TO") else {
        return;
    };
    assert!(
        std::env::args().any(|arg| arg == "--data-dir"),
        "The lifecycle check requires an isolated --data-dir"
    );
    let control = resident.control.clone();
    let quitting = resident.quitting.clone();
    std::thread::spawn(move || {
        let wait = |condition: &dyn Fn() -> bool| -> Result<()> {
            let deadline = Instant::now() + Duration::from_secs(10);
            while !condition() {
                ensure!(
                    Instant::now() < deadline,
                    "Timed out waiting for window lifecycle"
                );
                std::thread::sleep(Duration::from_millis(20));
            }
            Ok(())
        };
        let opened = || {
            let state = control.state.lock().unwrap();
            (state.opened, state.rendered && state.context.is_some())
        };
        let hidden = || control.state.lock().unwrap().context.is_none();
        let mut steps = Vec::new();
        let result = (|| -> Result<()> {
            ensure!(
                tray.version == (format!("Version {}", env!("CARGO_PKG_VERSION")), false),
                "Tray version must match the app and be disabled"
            );
            steps.push(format!("Disabled tray label: {}", tray.version.0));
            if background {
                std::thread::sleep(Duration::from_millis(150));
                ensure!(
                    hidden() && opened().0 == 0,
                    "Background startup opened a window"
                );
                tray.open();
                steps.push("Background startup kept only the tray; Open created a window".into());
            }
            wait(&|| opened() == (1, true)).context("initial rendered window")?;
            for cycle in 0..3 {
                let generation = opened().0;
                control.sender.send(Action::CloseWindow)?;
                wait(&hidden).context("titlebar close")?;
                ensure!(
                    !quitting.load(Ordering::Relaxed) && tray.alive.load(Ordering::Relaxed),
                    "Close destroyed the resident app or tray"
                );
                let before = tray.refreshes.load(Ordering::Relaxed);
                let limit = crate::model::MIN_LIMIT + cycle;
                handle.send(crate::engine::Command::SetLimit(limit));
                wait(&|| {
                    handle.view.lock().unwrap().limit == limit
                        && tray.refreshes.load(Ordering::Relaxed) > before
                })
                .context("hidden engine and tray refresh")?;
                ensure!(hidden(), "A background update reopened the window");
                tray.open();
                wait(&|| opened() == (generation + 1, true)).context("tray Open")?;
                control.sender.send(Action::CloseWindow)?;
                wait(&hidden).context("close before instance activation")?;
                let dir = handle.view.lock().unwrap().data_dir.clone();
                crate::instance::activate(&dir)?;
                wait(&|| opened() == (generation + 2, true))
                    .context("second-instance activation")?;
                steps.push(format!("cycle {cycle}: close kept tray/sync alive; tray Open and instance activation rendered new windows"));
            }
            control.sender.send(Action::CloseWindow)?;
            wait(&hidden).context("close before tray Quit")?;
            ensure!(
                !quitting.load(Ordering::Relaxed),
                "Close requested app shutdown"
            );
            steps.push("Tray Quit requested while hidden; caller verifies process exit".into());
            Ok(())
        })();
        let document = serde_json::json!({"passed": result.is_ok(), "backend": control.state.lock().unwrap().backend, "background": background, "steps": steps, "error": result.err().map(|error| format!("{error:#}"))});
        std::fs::write(report, serde_json::to_vec_pretty(&document).unwrap()).unwrap();
        tray.quit();
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn close_detaches_the_window_and_open_reaches_the_resident_app() {
        let resident = Resident::new(true);
        resident.control.attach(egui::Context::default());
        resident.control.detach();
        resident.control.refresh(false);
        resident.wake.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(!resident.take_open_request());
        assert!(!resident.quitting.load(Ordering::Relaxed));
        resident.control.open();
        resident.wake.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(resident.take_open_request());
        resident.control.attach(egui::Context::default());
        assert!(!resident.take_open_request());
        resident.control.detach();
        resident.control.quit();
        resident.wake.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(resident.quitting.load(Ordering::Relaxed));
    }

    #[test]
    fn pending_open_during_window_creation_is_fulfilled_once() {
        let resident = Resident::new(true);
        resident.control.open();
        resident.wake.recv_timeout(Duration::from_secs(2)).unwrap();
        resident.control.attach(egui::Context::default());
        assert!(!resident.take_open_request());
    }
}
