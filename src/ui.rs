use crate::{
    engine::{Command, Handle, PeerView},
    model::{format_bytes, HistoryItem, Kind, Status, DEFAULT_LIMIT, MAX_LIMIT, MIN_LIMIT},
};
use eframe::egui::{self, Color32, RichText, Stroke, Vec2};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem},
    Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent,
};
use uuid::Uuid;

const ACCENT: Color32 = Color32::from_rgb(109, 224, 193);
const MUTED: Color32 = Color32::from_rgb(151, 163, 184);

#[derive(PartialEq)]
enum Tab {
    Clipboard,
    Devices,
    Settings,
}

struct Settings<'a> {
    limit: u64,
    ask_all: bool,
    paused: bool,
    autostart: bool,
    clipboard: &'a str,
    dir: &'a std::path::Path,
}

pub struct App {
    handle: Handle,
    tab: Tab,
    _tray: Option<TrayIcon>,
    textures: HashMap<Uuid, egui::TextureHandle>,
    address: String,
    limit: f64,
    quitting: Arc<AtomicBool>,
    attention: Arc<AtomicBool>,
    window_control: crate::window::WindowControl,
    last_indicator: Option<crate::model::ClipboardNotice>,
    settling_frames: u8,
    #[cfg(feature = "ui-test")]
    preview_frames: u32,
    #[cfg(all(target_os = "windows", feature = "ui-test"))]
    lifecycle_frames: Arc<std::sync::atomic::AtomicU64>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, handle: Handle) -> Self {
        handle.set_notification_sender(crate::notifications::start());
        let mut visuals = egui::Visuals::dark();
        visuals.panel_fill = Color32::from_rgb(18, 23, 34);
        visuals.window_fill = visuals.panel_fill;
        visuals.selection.bg_fill = Color32::from_rgb(38, 94, 84);
        visuals.widgets.active.bg_fill = Color32::from_rgb(48, 102, 90);
        cc.egui_ctx.set_visuals(visuals);
        cc.egui_ctx.style_mut(|style| {
            style.spacing.item_spacing = Vec2::new(10.0, 10.0);
            style.spacing.button_padding = Vec2::new(12.0, 7.0);
        });
        let quitting = Arc::new(AtomicBool::new(false));
        let attention = Arc::new(AtomicBool::new(false));
        let window_control =
            crate::window::WindowControl::new(cc, quitting.clone(), attention.clone());
        let wake = window_control.clone();
        handle.set_notifier(Arc::new(move |show| {
            wake.refresh(show);
        }));
        let tray = build_tray(&handle, &window_control)
            .map_err(|error| tracing::warn!("Tray unavailable: {error}"))
            .ok();
        let limit = handle.view.lock().unwrap().limit as f64;
        #[cfg(feature = "ui-test")]
        cc.egui_ctx
            .request_repaint_after(Duration::from_millis(100));
        if tray.is_none() {
            cc.egui_ctx
                .send_viewport_cmd(egui::ViewportCommand::Visible(true));
        }
        #[cfg(all(target_os = "windows", feature = "ui-test"))]
        let lifecycle_frames = Arc::new(std::sync::atomic::AtomicU64::new(0));
        #[cfg(all(target_os = "windows", feature = "ui-test"))]
        crate::window::start_lifecycle_test(
            cc,
            window_control.clone(),
            handle.clone(),
            lifecycle_frames.clone(),
        );
        Self {
            handle,
            tab: if cfg!(feature = "ui-test")
                && std::env::var("DUET_PREVIEW_TAB").is_ok_and(|value| value == "settings")
            {
                Tab::Settings
            } else {
                Tab::Clipboard
            },
            _tray: tray,
            textures: HashMap::new(),
            address: String::new(),
            limit,
            quitting,
            attention,
            window_control,
            last_indicator: None,
            settling_frames: 0,
            #[cfg(feature = "ui-test")]
            preview_frames: 0,
            #[cfg(all(target_os = "windows", feature = "ui-test"))]
            lifecycle_frames,
        }
    }

    fn history(&mut self, ui: &mut egui::Ui, items: &[HistoryItem]) {
        if items.is_empty() {
            ui.add_space(45.0);
            ui.label(RichText::new("Ready when you copy").size(25.0));
            ui.label(
                RichText::new(
                    "Pair your computers in Devices, then copy text, an image, or a file as usual.",
                )
                .color(MUTED),
            );
            ui.add_space(10.0);
            if ui.button("Pair a computer").clicked() {
                self.tab = Tab::Devices;
            }
            return;
        }
        for item in items {
            egui::Frame::new()
                .fill(Color32::from_rgb(27, 34, 48))
                .corner_radius(12.0)
                .inner_margin(16.0)
                .show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.horizontal(|ui| {
                        let label = match item.offer.kind {
                            Kind::Text => "TEXT",
                            Kind::Image => "IMAGE",
                            Kind::Files => "FILE",
                        };
                        ui.label(RichText::new(label).size(10.0).color(ACCENT).strong());
                        ui.label(
                            RichText::new(format!(
                                "{}  ·  {}",
                                item.device,
                                format_bytes(item.offer.total)
                            ))
                            .small()
                            .color(MUTED),
                        );
                    });
                    ui.label(RichText::new(&item.offer.title).size(17.0).strong());
                    match item.status {
                        Status::AwaitingApproval => {
                            ui.label("Receive this copy into your clipboard?");
                            if item.offer.parts.len() > 1 {
                                for part in item.offer.parts.iter().take(5) {
                                    ui.label(
                                        RichText::new(format!(
                                            "{}  ·  {}",
                                            part.name,
                                            format_bytes(part.size)
                                        ))
                                        .small()
                                        .color(MUTED),
                                    );
                                }
                            }
                            ui.horizontal(|ui| {
                                if ui
                                    .add(
                                        egui::Button::new(
                                            RichText::new("Receive")
                                                .color(Color32::from_rgb(10, 35, 29)),
                                        )
                                        .fill(ACCENT),
                                    )
                                    .clicked()
                                {
                                    self.handle.send(Command::Receive(item.offer.id));
                                }
                                if ui.button("Decline").clicked() {
                                    self.handle.send(Command::Decline(item.offer.id));
                                }
                            });
                        }
                        Status::Receiving => {
                            ui.horizontal(|ui| {
                                progress_ring(
                                    ui,
                                    item.transferred as f32 / item.offer.total.max(1) as f32,
                                );
                                ui.vertical(|ui| {
                                    ui.label(format!(
                                        "{} / {}",
                                        format_bytes(item.transferred),
                                        format_bytes(item.offer.total)
                                    ));
                                    let label = if let (Some(rate), Some(eta)) =
                                        (item.rate(), item.eta())
                                    {
                                        format!(
                                            "{}/s  ·  {} remaining",
                                            format_bytes(rate as u64),
                                            format_duration(eta)
                                        )
                                    } else {
                                        "Estimating time remaining…".into()
                                    };
                                    ui.label(RichText::new(label).small().color(MUTED));
                                });
                            });
                            if ui.button("Cancel transfer").clicked() {
                                self.handle.send(Command::Cancel(item.offer.id));
                            }
                        }
                        Status::Ready | Status::Local => {
                            if let Some(text) = &item.preview {
                                egui::ScrollArea::vertical()
                                    .max_height(130.0)
                                    .id_salt(item.offer.id)
                                    .show(ui, |ui| {
                                        ui.label(text);
                                    });
                            }
                            if item.offer.kind == Kind::Image && !item.paths.is_empty() {
                                if let std::collections::hash_map::Entry::Vacant(entry) =
                                    self.textures.entry(item.offer.id)
                                {
                                    if let Ok(image) = image::open(&item.paths[0]) {
                                        let image = image.thumbnail(480, 240).to_rgba8();
                                        let size =
                                            [image.width() as usize, image.height() as usize];
                                        let image = egui::ColorImage::from_rgba_unmultiplied(
                                            size,
                                            image.as_raw(),
                                        );
                                        entry.insert(ui.ctx().load_texture(
                                            item.offer.id.to_string(),
                                            image,
                                            egui::TextureOptions::LINEAR,
                                        ));
                                    }
                                }
                                if let Some(texture) = self.textures.get(&item.offer.id) {
                                    ui.image((texture.id(), texture.size_vec2()));
                                }
                            }
                            if item.offer.kind == Kind::Files {
                                for part in item.offer.parts.iter().take(8) {
                                    ui.horizontal(|ui| {
                                        file_icon(ui);
                                        ui.label(&part.name);
                                        ui.label(
                                            RichText::new(format_bytes(part.size))
                                                .small()
                                                .color(MUTED),
                                        );
                                    });
                                }
                            }
                            ui.horizontal(|ui| {
                                let label = if item.applied {
                                    if item.status == Status::Local {
                                        "Copied here"
                                    } else {
                                        "Ready to paste"
                                    }
                                } else {
                                    "Saved · current clipboard kept"
                                };
                                ui.label(RichText::new(label).small().color(ACCENT));
                                if ui.small_button("Copy to clipboard").clicked() {
                                    self.handle.send(Command::Copy(item.offer.id));
                                }
                            });
                        }
                        Status::Failed(ref reason) => {
                            ui.colored_label(Color32::from_rgb(255, 170, 150), reason);
                            if ui.button("Retry").clicked() {
                                self.handle.send(Command::Receive(item.offer.id));
                            }
                        }
                        Status::Cancelled => {
                            ui.label(RichText::new("Transfer cancelled").color(MUTED));
                            if ui.button("Retry").clicked() {
                                self.handle.send(Command::Receive(item.offer.id));
                            }
                        }
                        Status::Declined => {
                            ui.label(
                                RichText::new("Download declined on this computer").color(MUTED),
                            );
                        }
                    }
                });
            ui.add_space(3.0);
        }
        self.textures
            .retain(|id, _| items.iter().any(|i| i.offer.id == *id));
    }

    fn devices(&mut self, ui: &mut egui::Ui, peers: &[PeerView], port: u16) {
        ui.label(RichText::new("Your computers").size(22.0).strong());
        ui.label(
            RichText::new("Pair once. Each computer sends and receives directly.").color(MUTED),
        );
        if peers.is_empty() {
            ui.label("Looking for nearby computers… Open this app on your other computer.");
        }
        for peer in peers {
            egui::Frame::new()
                .fill(Color32::from_rgb(27, 34, 48))
                .corner_radius(10.0)
                .inner_margin(14.0)
                .show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.label(RichText::new(&peer.name).strong().size(17.0));
                    if peer.connected && !peer.trusted {
                        ui.label("Check that this code matches on BOTH computers:");
                        ui.label(
                            RichText::new(&peer.code)
                                .monospace()
                                .size(25.0)
                                .color(ACCENT),
                        );
                        ui.label(
                            RichText::new("Only confirm for a computer you recognize.")
                                .small()
                                .color(MUTED),
                        );
                        ui.horizontal(|ui| {
                            if ui.button("Codes match · Pair").clicked() {
                                self.handle.send(Command::Trust(peer.id.clone()));
                            }
                            if ui.button("Reject").clicked() {
                                self.handle.send(Command::Forget(peer.id.clone()));
                            }
                        });
                    } else if peer.ready {
                        ui.colored_label(ACCENT, "Connected · encrypted");
                    } else if peer.connected {
                        ui.label("Waiting for approval on the other computer…");
                        ui.label(RichText::new(&peer.code).monospace().color(ACCENT));
                    } else {
                        ui.label(
                            RichText::new(if peer.trusted {
                                "Paired · offline"
                            } else {
                                "Nearby · not paired"
                            })
                            .color(MUTED),
                        );
                        if peer.address.is_some()
                            && ui
                                .button(if peer.trusted {
                                    "Reconnect"
                                } else {
                                    "Connect and pair"
                                })
                                .clicked()
                        {
                            self.handle.send(Command::Connect(peer.id.clone()));
                        }
                    }
                    if peer.trusted && ui.small_button("Forget device").clicked() {
                        self.handle.send(Command::Forget(peer.id.clone()));
                    }
                });
        }
        ui.add_space(12.0);
        ui.separator();
        ui.label("Connect by address");
        ui.label(RichText::new(format!("Use the other computer's LAN address and its displayed port. This computer's port is {port}.")).small().color(MUTED));
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.address)
                    .hint_text("192.168.1.20:54321")
                    .desired_width(280.0),
            );
            if ui.button("Connect").clicked() {
                self.handle
                    .send(Command::ConnectAddress(self.address.trim().to_owned()));
            }
        });
    }

    fn settings(&mut self, ui: &mut egui::Ui, settings: Settings<'_>) {
        let Settings {
            limit,
            ask_all,
            paused,
            autostart,
            clipboard,
            dir,
        } = settings;
        let mut ask = ask_all;
        if ui
            .checkbox(&mut ask, "Ask before receiving anything")
            .changed()
        {
            self.handle.send(Command::SetAcceptAll(ask));
        }
        ui.label(RichText::new("Every incoming text, image, or file waits for your approval. Your outgoing copies still share normally.").small().color(MUTED));
        ui.add_space(12.0);
        ui.label(
            RichText::new("Automatic download limit")
                .size(22.0)
                .strong(),
        );
        ui.label(RichText::new("This computer only").color(ACCENT));
        ui.label(if ask_all { "Automatic downloads are off while every incoming copy requires approval." } else { "Copies up to this total size download automatically. Larger copies ask before downloading." });
        ui.add_space(10.0);
        let response = ui.add_enabled(
            !ask_all,
            egui::Slider::new(&mut self.limit, MIN_LIMIT as f64..=MAX_LIMIT as f64)
                .logarithmic(true)
                .custom_formatter(|value, _| format_bytes(value as u64)),
        );
        if response.drag_stopped() || (response.changed() && !response.dragged()) {
            self.handle.send(Command::SetLimit(self.limit as u64));
        }
        if !response.dragged() && !response.changed() {
            self.limit = limit as f64;
        }
        ui.horizontal(|ui| {
            ui.label(RichText::new("500 KB").small().color(MUTED));
            ui.label(RichText::new("—").color(MUTED));
            ui.label(RichText::new("1 GB").small().color(MUTED));
        });
        if ui.small_button("Reset to 10 MB").clicked() {
            self.limit = DEFAULT_LIMIT as f64;
            self.handle.send(Command::SetLimit(DEFAULT_LIMIT));
        }
        ui.add_space(15.0);
        ui.separator();
        let mut enabled = autostart;
        if ui.checkbox(&mut enabled, "Start at login").changed() {
            self.handle.send(Command::SetAutostart(enabled));
        }
        let mut is_paused = paused;
        if ui
            .checkbox(&mut is_paused, "Pause clipboard sharing")
            .changed()
        {
            self.handle.send(Command::SetPaused(is_paused));
        }
        ui.label(RichText::new(clipboard).small().color(MUTED));
        ui.add_space(12.0);
        if ui.button("Clear recent history").clicked() {
            self.handle.send(Command::ClearHistory);
        }
        ui.label(
            RichText::new(format!("Local storage: {}", dir.display()))
                .small()
                .color(MUTED),
        );
        ui.label(
            RichText::new(
                "File downloads are copies. Your original files stay on the source computer.",
            )
            .small()
            .color(MUTED),
        );
        ui.add_space(15.0);
        if ui.button("Quit Duet Clipboard").clicked() {
            self.window_control.quit();
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        #[cfg(all(target_os = "windows", feature = "ui-test"))]
        self.lifecycle_frames.fetch_add(1, Ordering::Relaxed);
        if self.settling_frames < 8 {
            self.settling_frames += 1;
            ctx.request_repaint_after(Duration::from_millis(16));
        }
        if ctx.input(|i| i.viewport().close_requested())
            && !self.quitting.load(Ordering::Relaxed)
            && self._tray.is_some()
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        }
        // Snapshot the model so drawing never holds the background engine's lock.
        let (
            name,
            peers,
            history,
            notices,
            limit,
            ask_all,
            paused,
            autostart,
            port,
            clipboard,
            dir,
            incoming,
        ) = {
            let view = self.handle.view.lock().unwrap();
            (
                view.name.clone(),
                view.peers.clone(),
                view.history.clone(),
                view.notices.clone(),
                view.limit,
                view.ask_before_receiving,
                view.paused,
                view.autostart,
                view.port,
                view.clipboard.clone(),
                view.data_dir.clone(),
                view.latest_incoming.clone(),
            )
        };
        let indicator = history
            .iter()
            .find(|item| item.status == Status::AwaitingApproval)
            .map(|item| crate::model::ClipboardNotice {
                event: item.offer.id,
                kind: item.offer.kind.clone(),
                phase: crate::model::NoticePhase::ApprovalNeeded,
            })
            .or(incoming);
        if indicator != self.last_indicator {
            if let (Some(tray), Some(notice)) = (&self._tray, &indicator) {
                if let Ok(icon) = Icon::from_rgba(status_icon(&notice.phase), 32, 32) {
                    let _ = tray.set_icon(Some(icon));
                }
                let label = match notice.kind {
                    Kind::Text => "Text",
                    Kind::Image => "Image",
                    Kind::Files => "File",
                };
                let _ = tray.set_tooltip(Some(format!(
                    "New clipboard · {label} · {}",
                    crate::notifications::message(notice)
                )));
            }
            if indicator.is_none() {
                if let Some(tray) = &self._tray {
                    let _ = tray.set_icon(Icon::from_rgba(icon_rgba(), 32, 32).ok());
                    let _ = tray.set_tooltip(Some("Duet Clipboard"));
                }
            }
            self.last_indicator = indicator;
        }
        if self.attention.swap(false, Ordering::Relaxed) {
            self.tab = if peers.iter().any(|p| p.connected && !p.trusted) {
                Tab::Devices
            } else {
                Tab::Clipboard
            };
        }
        egui::TopBottomPanel::top("top")
            .frame(
                egui::Frame::new()
                    .inner_margin(20.0)
                    .fill(Color32::from_rgb(18, 23, 34)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(RichText::new("Duet Clipboard").size(27.0).strong());
                        ui.label(
                            RichText::new(format!(
                                "{}  ·  {}",
                                name,
                                if paused {
                                    "Sharing paused"
                                } else {
                                    "Your clipboard, everywhere."
                                }
                            ))
                            .small()
                            .color(MUTED),
                        );
                    });
                });
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    let pending = history
                        .iter()
                        .filter(|item| item.status == Status::AwaitingApproval)
                        .count();
                    ui.selectable_value(
                        &mut self.tab,
                        Tab::Clipboard,
                        if pending == 0 {
                            "Clipboard".to_owned()
                        } else {
                            format!("Clipboard · {pending} pending")
                        },
                    );
                    ui.selectable_value(
                        &mut self.tab,
                        Tab::Devices,
                        format!("Devices · {}", peers.iter().filter(|p| p.ready).count()),
                    );
                    ui.selectable_value(&mut self.tab, Tab::Settings, "Settings");
                });
            });
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .inner_margin(20.0)
                    .fill(Color32::from_rgb(18, 23, 34)),
            )
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        if let Some(notice) = notices.back() {
                            egui::Frame::new()
                                .fill(Color32::from_rgb(61, 44, 39))
                                .corner_radius(8.0)
                                .inner_margin(10.0)
                                .show(ui, |ui| {
                                    ui.horizontal_wrapped(|ui| {
                                        ui.label(
                                            RichText::new(notice)
                                                .color(Color32::from_rgb(255, 204, 170)),
                                        );
                                        if ui.small_button("Dismiss").clicked() {
                                            self.handle
                                                .send(Command::DismissNotice(notice.clone()));
                                        }
                                    });
                                });
                            ui.add_space(8.0);
                        }
                        match self.tab {
                            Tab::Clipboard => self.history(ui, &history),
                            Tab::Devices => self.devices(ui, &peers, port),
                            Tab::Settings => self.settings(
                                ui,
                                Settings {
                                    limit,
                                    ask_all,
                                    paused,
                                    autostart,
                                    clipboard: &clipboard,
                                    dir: &dir,
                                },
                            ),
                        }
                    });
            });
        if history.iter().any(|item| item.status == Status::Receiving) {
            ctx.request_repaint_after(Duration::from_millis(200));
        }
        #[cfg(feature = "ui-test")]
        if let Ok(path) = std::env::var("DUET_SCREENSHOT_TO") {
            self.preview_frames += 1;
            if self.preview_frames == 8 {
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
            }
            for event in ctx.input(|input| input.events.clone()) {
                if let egui::Event::Screenshot { image, .. } = event {
                    let bytes: Vec<u8> = image
                        .pixels
                        .iter()
                        .flat_map(|pixel| pixel.to_array())
                        .collect();
                    if let Some(pixels) = image::RgbaImage::from_raw(
                        image.size[0] as u32,
                        image.size[1] as u32,
                        bytes,
                    ) {
                        let _ = pixels.save(&path);
                    }
                    self.quitting.store(true, Ordering::Relaxed);
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
            ctx.request_repaint_after(Duration::from_millis(30));
        }
    }
}

impl Drop for App {
    fn drop(&mut self) {
        self.window_control.stop();
    }
}

fn build_tray(handle: &Handle, control: &crate::window::WindowControl) -> anyhow::Result<TrayIcon> {
    let menu = Menu::new();
    let open = MenuItem::new("Open clipboard", true, None);
    let pause = MenuItem::new("Pause / resume sharing", true, None);
    let quit = MenuItem::new("Quit", true, None);
    menu.append_items(&[&open, &pause, &quit])?;
    let open_id = open.id().clone();
    let pause_id = pause.id().clone();
    let quit_id = quit.id().clone();
    let window = control.clone();
    let bus = handle.clone();
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        if event.id == open_id {
            window.open();
        }
        if event.id == pause_id {
            let paused = bus.view.lock().unwrap().paused;
            bus.send(Command::SetPaused(!paused));
        }
        if event.id == quit_id {
            window.quit();
        }
    }));
    let window = control.clone();
    TrayIconEvent::set_event_handler(Some(move |event: TrayIconEvent| {
        if matches!(
            event,
            TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } | TrayIconEvent::DoubleClick {
                button: MouseButton::Left,
                ..
            }
        ) {
            window.open();
        }
    }));
    Ok(TrayIconBuilder::new()
        .with_tooltip("Duet Clipboard")
        .with_icon(Icon::from_rgba(icon_rgba(), 32, 32)?)
        .with_menu(Box::new(menu))
        .with_menu_on_left_click(false)
        .build()?)
}

pub fn icon_rgba() -> Vec<u8> {
    image::load_from_memory(include_bytes!("../packaging/icons/tray-32.png"))
        .expect("embedded tray icon is a valid PNG")
        .into_rgba8()
        .into_raw()
}

fn status_icon(phase: &crate::model::NoticePhase) -> Vec<u8> {
    let mut pixels = image::RgbaImage::from_raw(32, 32, icon_rgba()).unwrap();
    let color = match phase {
        crate::model::NoticePhase::ApprovalNeeded => [255, 191, 83, 255],
        crate::model::NoticePhase::Receiving => [144, 154, 255, 255],
        crate::model::NoticePhase::Declined | crate::model::NoticePhase::Cancelled => {
            [151, 163, 184, 255]
        }
        crate::model::NoticePhase::Failed => [255, 140, 120, 255],
        _ => [109, 224, 193, 255],
    };
    for y in 21..32u32 {
        for x in 21..32u32 {
            let radius = (x as i32 - 26).pow(2) + (y as i32 - 26).pow(2);
            if radius <= 25 {
                pixels.put_pixel(
                    x,
                    y,
                    image::Rgba(if radius > 16 {
                        [18, 23, 34, 255]
                    } else {
                        color
                    }),
                );
            }
        }
    }
    pixels.into_raw()
}

fn progress_ring(ui: &mut egui::Ui, fraction: f32) {
    let fraction = fraction.clamp(0.0, 1.0);
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(72.0), egui::Sense::hover());
    ui.painter().circle_stroke(
        rect.center(),
        29.0,
        Stroke::new(5.0_f32, Color32::from_rgb(48, 60, 76)),
    );
    let points = (0..=64)
        .map(|i| {
            let angle =
                -std::f32::consts::FRAC_PI_2 + std::f32::consts::TAU * fraction * i as f32 / 64.0;
            rect.center() + Vec2::angled(angle) * 29.0
        })
        .collect::<Vec<_>>();
    ui.painter()
        .add(egui::Shape::line(points, Stroke::new(5.0_f32, ACCENT)));
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        format!("{:.0}%", fraction * 100.0),
        egui::FontId::proportional(14.0),
        Color32::WHITE,
    );
}

fn file_icon(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(20.0, 25.0), egui::Sense::hover());
    ui.painter().rect_stroke(
        rect.shrink(2.0),
        2.0,
        Stroke::new(1.2_f32, MUTED),
        egui::StrokeKind::Inside,
    );
    for y in [10.0, 15.0] {
        ui.painter().line_segment(
            [rect.min + Vec2::new(6.0, y), rect.min + Vec2::new(14.0, y)],
            Stroke::new(1.0_f32, MUTED),
        );
    }
}

fn format_duration(seconds: f64) -> String {
    if seconds >= 3600.0 {
        format!("{}h {}m", seconds as u64 / 3600, seconds as u64 / 60 % 60)
    } else if seconds >= 60.0 {
        format!("{}m {}s", seconds as u64 / 60, seconds as u64 % 60)
    } else {
        format!("{}s", seconds.max(1.0) as u64)
    }
}
