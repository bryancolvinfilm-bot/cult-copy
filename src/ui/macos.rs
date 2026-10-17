use super::{egui, RichText, ACCENT, MUTED};
use crate::handoff::HandoffStatus;
use std::{
    sync::mpsc::{self, Receiver, TryRecvError},
    time::{Duration, Instant},
};

const CHECK_INTERVAL: Duration = Duration::from_secs(5);

#[derive(Default)]
pub(super) struct MacClipboardSetup {
    status: Option<HandoffStatus>,
    pending: Option<Receiver<HandoffStatus>>,
    last_check: Option<Instant>,
    was_focused: bool,
    open_error: Option<String>,
}

impl MacClipboardSetup {
    fn enabled(&self) -> bool {
        cfg!(target_os = "macos") || std::env::var_os("DUET_PREVIEW_MAC_HANDOFF").is_some()
    }

    pub(super) fn update(&mut self, ctx: &egui::Context) {
        if !self.enabled() {
            return;
        }
        if let Some(receiver) = &self.pending {
            match receiver.try_recv() {
                Ok(status) => {
                    self.status = Some(status);
                    self.pending = None;
                }
                Err(TryRecvError::Disconnected) => {
                    self.status = Some(HandoffStatus::Unknown);
                    self.pending = None;
                }
                Err(TryRecvError::Empty) => {}
            }
        }
        let focused = ctx.input(|input| input.viewport().focused.unwrap_or(false));
        let returned = focused && !self.was_focused;
        self.was_focused = focused;
        if self.last_check.is_none()
            || returned
            || (focused
                && self
                    .last_check
                    .is_some_and(|time| time.elapsed() >= CHECK_INTERVAL))
        {
            self.recheck(ctx);
        }
        if focused {
            ctx.request_repaint_after(CHECK_INTERVAL);
        }
    }

    fn recheck(&mut self, ctx: &egui::Context) {
        if self.pending.is_some() {
            return;
        }
        let (sender, receiver) = mpsc::channel();
        let wake = ctx.clone();
        // Preference services may touch disk. Keep that work off the UI and
        // clipboard-engine threads, and don't poll while the app is hidden.
        match std::thread::Builder::new()
            .name("handoff-check".into())
            .spawn(move || {
                #[cfg(target_os = "macos")]
                let status = crate::handoff::status();
                #[cfg(not(target_os = "macos"))]
                let status = match std::env::var("DUET_PREVIEW_MAC_HANDOFF").as_deref() {
                    Ok("on") => HandoffStatus::On,
                    Ok("off") => HandoffStatus::Off,
                    _ => HandoffStatus::Unknown,
                };
                let _ = sender.send(status);
                wake.request_repaint();
            }) {
            Ok(_) => self.pending = Some(receiver),
            Err(_) => self.status = Some(HandoffStatus::Unknown),
        }
        self.last_check = Some(Instant::now());
    }

    fn open_button(&mut self, ui: &mut egui::Ui) {
        if ui.button("Open Handoff settings").clicked() {
            #[cfg(target_os = "macos")]
            {
                self.open_error = crate::handoff::open_settings()
                    .err()
                    .map(|error| error.to_string());
            }
        }
        if self.open_error.is_some() {
            ui.label("Couldn't open System Settings. Follow the steps below to open it manually.");
        }
    }

    pub(super) fn banner(&mut self, ui: &mut egui::Ui) {
        if !self.enabled() || self.status.is_none() || self.status == Some(HandoffStatus::Off) {
            return;
        }
        egui::Frame::new()
            .fill(egui::Color32::from_rgb(44, 39, 35))
            .corner_radius(8.0)
            .inner_margin(12.0)
            .show(ui, |ui| {
                ui.label(RichText::new("Mac-to-Mac clipboard sharing").strong());
                ui.label(if self.status == Some(HandoffStatus::On) {
                    "Apple's Handoff appears on and may compete with Duet. Turn it off on each Mac using Duet."
                } else {
                    "To prevent Apple's clipboard sharing from competing with Duet, check that Handoff is off on each Mac."
                });
                self.open_button(ui);
                ui.label(RichText::new("Turn off \"Allow Handoff between this Mac and your iCloud devices.\" This also turns off handing off activities between devices.").small().color(MUTED));
                if self.open_error.is_some() {
                    manual_route(ui);
                }
            });
        ui.add_space(12.0);
    }

    pub(super) fn settings(&mut self, ui: &mut egui::Ui) {
        if !self.enabled() {
            return;
        }
        ui.label(RichText::new("Mac clipboard setup").size(22.0).strong());
        let (label, color) = match self.status {
            Some(HandoffStatus::On) => (
                "Handoff appears on · Apple clipboard sharing may compete with Duet",
                egui::Color32::from_rgb(255, 204, 170),
            ),
            Some(HandoffStatus::Off) => ("Handoff appears off on this Mac", ACCENT),
            Some(HandoffStatus::Unknown) => ("Couldn't confirm the Handoff setting", MUTED),
            None => ("Checking Handoff…", MUTED),
        };
        ui.label(RichText::new(label).color(color));
        ui.label("Let Duet handle clipboard sharing by turning off \"Allow Handoff between this Mac and your iCloud devices\" on every Mac using Duet.");
        ui.horizontal_wrapped(|ui| {
            self.open_button(ui);
            if ui
                .add_enabled(self.pending.is_none(), egui::Button::new("Check again"))
                .clicked()
            {
                self.recheck(ui.ctx());
            }
        });
        manual_route(ui);
        ui.label(RichText::new("Turning Handoff off also disables handing off activities such as Safari pages. Duet doesn't change this switch. The status check is best effort; verify the switch in System Settings. It refreshes when you return here.").small().color(MUTED));
        ui.add_space(12.0);
        ui.separator();
        ui.add_space(12.0);
    }
}

fn manual_route(ui: &mut egui::Ui) {
    ui.label(RichText::new("System Settings > General > AirDrop & Handoff (or AirDrop & Continuity). On macOS Monterey: System Preferences > General.").small().color(MUTED));
}
