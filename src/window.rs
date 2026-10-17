//! Dispatch window requests outside native tray callbacks. On Windows, hidden
//! windows do not receive the WM_PAINT that eframe needs to apply viewport
//! commands. Explicitly posting it also keeps the hidden tray indicator current.
use eframe::egui::{self, ViewportCommand, ViewportId};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc,
};

enum Action {
    Refresh(bool),
    Open,
    Quit,
    Stop,
}

#[derive(Clone)]
pub struct WindowControl {
    sender: mpsc::Sender<Action>,
}

impl WindowControl {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        quitting: Arc<AtomicBool>,
        attention: Arc<AtomicBool>,
    ) -> Self {
        let context = cc.egui_ctx.clone();
        #[cfg(target_os = "windows")]
        let native = NativeWindow::from_creation(cc);
        let (sender, receiver) = mpsc::channel();
        std::thread::Builder::new()
            .name("clipboard-window".into())
            .spawn(move || {
                while let Ok(action) = receiver.recv() {
                    let command = |command| context.send_viewport_cmd_to(ViewportId::ROOT, command);
                    match action {
                        Action::Refresh(show) => {
                            if show {
                                attention.store(true, Ordering::Relaxed);
                                command(ViewportCommand::Visible(true));
                                command(ViewportCommand::RequestUserAttention(
                                    egui::UserAttentionType::Informational,
                                ));
                            }
                        }
                        Action::Open => {
                            #[cfg(target_os = "windows")]
                            if let Some(window) = native {
                                window.restore();
                            }
                            command(ViewportCommand::Visible(true));
                            command(ViewportCommand::Minimized(false));
                            command(ViewportCommand::Focus);
                        }
                        Action::Quit => {
                            quitting.store(true, Ordering::Relaxed);
                            #[cfg(target_os = "windows")]
                            if let Some(window) = native {
                                // WM_CLOSE sets the close event before the explicit
                                // WM_PAINT below, even while the window is hidden.
                                window.close();
                            } else {
                                command(ViewportCommand::Close);
                            }
                            #[cfg(not(target_os = "windows"))]
                            command(ViewportCommand::Close);
                        }
                        Action::Stop => break,
                    }
                    context.request_repaint_of(ViewportId::ROOT);
                    #[cfg(target_os = "windows")]
                    if let Some(window) = native {
                        window.redraw();
                    }
                }
            })
            .expect("Could not start the window dispatcher");
        Self { sender }
    }

    pub fn refresh(&self, show: bool) {
        let _ = self.sender.send(Action::Refresh(show));
    }
    pub fn open(&self) {
        let _ = self.sender.send(Action::Open);
    }
    pub fn quit(&self) {
        let _ = self.sender.send(Action::Quit);
    }
    pub fn stop(&self) {
        let _ = self.sender.send(Action::Stop);
    }
}

#[cfg(target_os = "windows")]
#[derive(Clone, Copy)]
struct NativeWindow(isize);

#[cfg(target_os = "windows")]
impl NativeWindow {
    fn from_creation(cc: &eframe::CreationContext<'_>) -> Option<Self> {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        match cc.window_handle().ok()?.as_raw() {
            RawWindowHandle::Win32(handle) => Some(Self(handle.hwnd.get())),
            _ => None,
        }
    }
    fn hwnd(self) -> windows_sys::Win32::Foundation::HWND {
        self.0 as windows_sys::Win32::Foundation::HWND
    }
    fn post(self, message: u32) {
        use windows_sys::Win32::UI::WindowsAndMessaging::{GetWindowThreadProcessId, PostMessageW};
        // The handle came from this eframe root window. Check ownership before
        // posting so a late request cannot affect a reused handle in another app.
        unsafe {
            let mut owner = 0;
            GetWindowThreadProcessId(self.hwnd(), &mut owner);
            if owner == std::process::id() {
                PostMessageW(self.hwnd(), message, 0, 0);
            }
        }
    }
    fn redraw(self) {
        self.post(windows_sys::Win32::UI::WindowsAndMessaging::WM_PAINT);
    }
    fn close(self) {
        self.post(windows_sys::Win32::UI::WindowsAndMessaging::WM_CLOSE);
    }
    fn restore(self) {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            IsIconic, ShowWindowAsync, SW_RESTORE, SW_SHOW,
        };
        // This asynchronous call never waits on the GUI thread. Viewport Focus
        // is subsequently applied on that thread once the window is restored.
        unsafe {
            ShowWindowAsync(
                self.hwnd(),
                if IsIconic(self.hwnd()) != 0 {
                    SW_RESTORE
                } else {
                    SW_SHOW
                },
            );
        }
    }
}

#[cfg(all(target_os = "windows", feature = "ui-test"))]
pub fn start_lifecycle_test(
    cc: &eframe::CreationContext<'_>,
    control: WindowControl,
    handle: crate::engine::Handle,
    frames: Arc<std::sync::atomic::AtomicU64>,
) {
    use anyhow::{ensure, Context, Result};
    use std::time::{Duration, Instant};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        IsIconic, IsWindowVisible, ShowWindowAsync, SW_MINIMIZE,
    };
    let Ok(report) = std::env::var("DUET_LIFECYCLE_TEST_TO") else {
        return;
    };
    assert!(
        std::env::args().any(|arg| arg == "--data-dir"),
        "The lifecycle check requires an isolated --data-dir"
    );
    let native = NativeWindow::from_creation(cc).expect("Windows root handle");
    std::thread::spawn(move || {
        let mut steps = Vec::new();
        let wait = |condition: &dyn Fn() -> bool| -> Result<()> {
            let deadline = Instant::now() + Duration::from_secs(5);
            while !condition() {
                ensure!(
                    Instant::now() < deadline,
                    "Timed out waiting for window lifecycle"
                );
                std::thread::sleep(Duration::from_millis(25));
            }
            Ok(())
        };
        let visible = || unsafe { IsWindowVisible(native.hwnd()) != 0 };
        let result = (|| -> Result<()> {
            wait(&visible).context("initial window")?;
            // Let the initial settling repaints end. The regression occurs when
            // no screenshot or animation is continuously requesting frames.
            std::thread::sleep(Duration::from_secs(1));
            for cycle in 0..3 {
                native.close();
                wait(&|| !visible()).context("titlebar close to tray")?;
                steps.push(format!("cycle {cycle}: titlebar close hid the window"));
                let before = frames.load(Ordering::Relaxed);
                let limit = crate::model::MIN_LIMIT + cycle;
                handle.send(crate::engine::Command::SetLimit(limit));
                wait(&|| {
                    handle.view.lock().unwrap().limit == limit
                        && frames.load(Ordering::Relaxed) > before
                })
                .context("background worker and hidden-window refresh")?;
                ensure!(
                    !visible(),
                    "A background update unexpectedly showed the window"
                );
                steps.push(format!(
                    "cycle {cycle}: engine and tray updates ran while hidden"
                ));
                control.open();
                wait(&visible).context("tray Open")?;
                steps.push(format!("cycle {cycle}: tray Open restored the window"));
                native.close();
                wait(&|| !visible()).context("close before instance activation")?;
                let dir = handle.view.lock().unwrap().data_dir.clone();
                crate::instance::activate(&dir)?;
                wait(&visible).context("second-instance activation")?;
                ensure!(
                    unsafe { IsIconic(native.hwnd()) == 0 },
                    "Window remained minimized"
                );
                steps.push(format!(
                    "cycle {cycle}: instance activation restored the window"
                ));
            }
            unsafe { ShowWindowAsync(native.hwnd(), SW_MINIMIZE) };
            wait(&|| unsafe { IsIconic(native.hwnd()) != 0 }).context("minimize")?;
            control.open();
            wait(&|| visible() && unsafe { IsIconic(native.hwnd()) == 0 })
                .context("restore minimized window")?;
            steps.push("Open also restored a minimized window".into());
            native.close();
            wait(&|| !visible()).context("close before Quit")?;
            steps.push("Quit requested while hidden; caller must verify process exit".into());
            Ok(())
        })();
        let document = serde_json::json!({
            "passed": result.is_ok(),
            "steps": steps,
            "error": result.as_ref().err().map(|error| format!("{error:#}")),
        });
        std::fs::write(report, serde_json::to_vec_pretty(&document).unwrap()).unwrap();
        if result.is_err() {
            std::process::exit(2);
        }
        control.quit();
    });
}
