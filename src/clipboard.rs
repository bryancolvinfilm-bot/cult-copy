use crate::model::{Kind, TextPayload, MAX_INLINE};
use anyhow::{ensure, Context, Result};
use clipboard_rs::{
    common::RustImage, Clipboard, ClipboardContent, ClipboardContext, ClipboardHandler,
    ClipboardWatcher, ClipboardWatcherContext, ContentFormat,
};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::sync::mpsc;
use uuid::Uuid;

pub const MARKER: &str = "application/x-duet-clipboard-event";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Captured {
    Text(TextPayload),
    Image(Vec<u8>),
    Files(Vec<PathBuf>),
}

impl Captured {
    pub fn signature(&self) -> String {
        // This signature describes the local clipboard, including local file paths.
        // Wire integrity uses independent hashes of each file's actual contents.
        let mut hash = blake3::Hasher::new();
        hash.update(&bincode::serialize(self).expect("serializable clipboard"));
        if let Captured::Files(paths) = self {
            for path in paths {
                if let Ok(metadata) = std::fs::metadata(path) {
                    hash.update(&metadata.len().to_le_bytes());
                    if let Ok(modified) = metadata.modified().and_then(|time| {
                        time.duration_since(std::time::UNIX_EPOCH)
                            .map_err(std::io::Error::other)
                    }) {
                        hash.update(&modified.as_nanos().to_le_bytes());
                    }
                }
            }
        }
        hash.finalize().to_hex().to_string()
    }
}

pub struct Snapshot {
    pub content: Option<Captured>,
    pub marker: Option<Uuid>,
}

pub trait ClipboardAccess: Send {
    fn snapshot(&self) -> Result<Snapshot>;
    fn apply(&self, kind: &Kind, paths: &[PathBuf], id: Uuid) -> Result<()>;
}

impl ClipboardAccess for SystemClipboard {
    fn snapshot(&self) -> Result<Snapshot> {
        SystemClipboard::snapshot(self)
    }
    fn apply(&self, kind: &Kind, paths: &[PathBuf], id: Uuid) -> Result<()> {
        SystemClipboard::apply(self, kind, paths, id)
    }
}

pub struct SystemClipboard {
    context: ClipboardContext,
    _watcher: Option<clipboard_rs::WatcherShutdown>,
}

impl SystemClipboard {
    pub fn new(bus: mpsc::Sender<crate::engine::Command>) -> Result<Self> {
        let context =
            ClipboardContext::new().map_err(|e| anyhow::anyhow!("Clipboard unavailable: {e}"))?;
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        std::thread::Builder::new()
            .name("clipboard-watch".into())
            .spawn(move || {
                struct Handler(mpsc::Sender<crate::engine::Command>);
                impl ClipboardHandler for Handler {
                    fn on_clipboard_change(&mut self) {
                        // Preserve clipboard changes when file chunks temporarily fill
                        // the engine queue. This runs on the dedicated watcher thread.
                        let _ = self
                            .0
                            .blocking_send(crate::engine::Command::ClipboardChanged);
                    }
                }
                match ClipboardWatcherContext::new() {
                    Ok(mut watcher) => {
                        watcher.add_handler(Handler(bus));
                        let shutdown = watcher.get_shutdown_channel();
                        if ready_tx.send(Some(shutdown)).is_ok() {
                            watcher.start_watch();
                        }
                    }
                    Err(_) => {
                        let _ = ready_tx.send(None);
                    }
                }
            })?;
        let shutdown = ready_rx
            .recv_timeout(std::time::Duration::from_secs(2))
            .ok()
            .flatten();
        Ok(Self {
            context,
            _watcher: shutdown,
        })
    }

    pub fn snapshot(&self) -> Result<Snapshot> {
        let formats = self
            .context
            .available_formats()
            .map_err(|e| anyhow::anyhow!("Cannot read clipboard: {e}"))?;
        let marker = self
            .context
            .get_buffer(MARKER)
            .ok()
            .and_then(|data| Uuid::from_slice(&data).ok());
        // Respect the common Windows, Apple, and KDE sensitive-content conventions.
        let sensitive = formats.iter().any(|s| {
            matches!(
                s.as_str(),
                "ExcludeClipboardContentFromMonitorProcessing"
                    | "org.nspasteboard.ConcealedType"
                    | "org.nspasteboard.TransientType"
            )
        });
        let kde_sensitive = self
            .context
            .get_buffer("x-kde-passwordManagerHint")
            .ok()
            .is_some_and(|data| data == b"secret");
        if sensitive || kde_sensitive {
            return Ok(Snapshot {
                content: None,
                marker,
            });
        }
        let content = if self.context.has(ContentFormat::Files) {
            let files = self
                .context
                .get_files()
                .map_err(|e| anyhow::anyhow!("Cannot read copied files: {e}"))?;
            let paths = files
                .iter()
                .map(|value| {
                    if value.starts_with("file:") {
                        url::Url::parse(value)?
                            .to_file_path()
                            .map_err(|_| anyhow::anyhow!("Invalid file URL"))
                    } else {
                        Ok(PathBuf::from(value))
                    }
                })
                .collect::<Result<Vec<_>>>()?;
            (!paths.is_empty()).then_some(Captured::Files(paths))
        } else if self.context.has(ContentFormat::Image) {
            let image = self
                .context
                .get_image()
                .map_err(|e| anyhow::anyhow!("Cannot read image: {e}"))?;
            let (width, height) = image.get_size();
            ensure!(
                u64::from(width) * u64::from(height) <= 64_000_000,
                "Image is too large"
            );
            let bytes = image
                .to_png()
                .map_err(|e| anyhow::anyhow!("Cannot encode image: {e}"))?
                .get_bytes()
                .to_vec();
            ensure!(
                bytes.len() as u64 <= MAX_INLINE,
                "Image exceeds the 64 MB clipboard limit"
            );
            Some(Captured::Image(bytes))
        } else if self.context.has(ContentFormat::Text) {
            let text = self
                .context
                .get_text()
                .map_err(|e| anyhow::anyhow!("Cannot read text: {e}"))?;
            let html = self.context.get_html().ok().filter(|s| !s.is_empty());
            let rtf = self.context.get_rich_text().ok().filter(|s| !s.is_empty());
            ensure!(
                text.len() as u64
                    + html.as_ref().map_or(0, |s| s.len() as u64)
                    + rtf.as_ref().map_or(0, |s| s.len() as u64)
                    <= MAX_INLINE / 2,
                "Text exceeds the clipboard size limit"
            );
            Some(Captured::Text(TextPayload { text, html, rtf }))
        } else {
            None
        };
        Ok(Snapshot { content, marker })
    }

    pub fn description(&self) -> &'static str {
        #[cfg(target_os = "linux")]
        if matches!(self.context, ClipboardContext::X11(_))
            && std::env::var_os("WAYLAND_DISPLAY").is_some()
        {
            return "XWayland clipboard only. Full sync with native Wayland apps needs a desktop with data-control support, or an X11 session.";
        }
        if self._watcher.is_none() {
            "Clipboard reads are available, but automatic change monitoring could not start."
        } else {
            "Clipboard monitor running"
        }
    }

    pub fn apply(&self, kind: &Kind, paths: &[PathBuf], id: Uuid) -> Result<()> {
        ensure!(!paths.is_empty(), "No downloaded content is available");
        #[cfg(target_os = "macos")]
        if *kind == Kind::Files {
            return macos_files(paths, id);
        }
        let mut values = match kind {
            Kind::Text => {
                let payload = crate::storage::text_preview(&paths[0])?;
                let mut values = vec![ClipboardContent::Text(payload.text)];
                if let Some(html) = payload.html {
                    values.push(ClipboardContent::Html(html));
                }
                if let Some(rtf) = payload.rtf {
                    values.push(ClipboardContent::Rtf(rtf));
                }
                values
            }
            Kind::Image => {
                check_image(&paths[0])?;
                vec![ClipboardContent::Image(
                    clipboard_rs::RustImageData::from_path(
                        paths[0].to_str().context("Invalid image path")?,
                    )
                    .map_err(|e| anyhow::anyhow!("Cannot load image: {e}"))?,
                )]
            }
            Kind::Files => {
                #[cfg(any(target_os = "windows", target_os = "macos"))]
                let files = paths
                    .iter()
                    .map(|path| path.to_string_lossy().into_owned())
                    .collect();
                #[cfg(target_os = "linux")]
                let files = paths
                    .iter()
                    .map(|path| {
                        url::Url::from_file_path(path)
                            .map(|u| u.to_string())
                            .map_err(|_| anyhow::anyhow!("Invalid local file path"))
                    })
                    .collect::<Result<Vec<_>>>()?;
                #[cfg(target_os = "linux")]
                let extras = {
                    let copied = format!("copy\n{}", files.join("\n")).into_bytes();
                    vec![
                        ClipboardContent::Other(
                            "x-special/gnome-copied-files".into(),
                            copied.clone(),
                        ),
                        ClipboardContent::Other("x-special/nautilus-clipboard".into(), copied),
                        ClipboardContent::Other(
                            "application/x-kde-cutselection".into(),
                            b"0".to_vec(),
                        ),
                    ]
                };
                #[cfg(not(target_os = "linux"))]
                let extras: Vec<ClipboardContent> = Vec::new();
                let mut values = vec![ClipboardContent::Files(files)];
                values.extend(extras);
                values
            }
        };
        values.push(ClipboardContent::Other(
            MARKER.into(),
            id.as_bytes().to_vec(),
        ));
        self.context
            .set(values)
            .map_err(|e| anyhow::anyhow!("Cannot update clipboard: {e}"))?;
        Ok(())
    }
}

#[cfg(target_os = "macos")]
fn macos_files(paths: &[PathBuf], id: Uuid) -> Result<()> {
    use objc2::{rc::autoreleasepool, runtime::ProtocolObject};
    use objc2_app_kit::{NSPasteboard, NSPasteboardItem, NSPasteboardWriting};
    use objc2_foundation::{NSArray, NSString};
    autoreleasepool(|_| {
        let pasteboard = NSPasteboard::generalPasteboard();
        let mut items = Vec::new();
        for path in paths {
            let url =
                url::Url::from_file_path(path).map_err(|_| anyhow::anyhow!("Invalid file path"))?;
            let item = NSPasteboardItem::new();
            ensure!(
                item.setString_forType(
                    &NSString::from_str(url.as_str()),
                    &NSString::from_str("public.file-url")
                ),
                "Cannot prepare file clipboard"
            );
            let marker = id.as_bytes();
            // NSData copies the bytes before this stack frame returns.
            let data = unsafe {
                objc2_foundation::NSData::dataWithBytes_length(marker.as_ptr().cast(), marker.len())
            };
            item.setData_forType(&data, &NSString::from_str(MARKER));
            items.push(ProtocolObject::<dyn NSPasteboardWriting>::from_retained(
                item,
            ));
        }
        pasteboard.clearContents();
        ensure!(
            pasteboard.writeObjects(&NSArray::from_retained_slice(&items)),
            "Cannot write file clipboard"
        );
        Ok(())
    })
}

pub fn check_image(path: &Path) -> Result<()> {
    let reader = image::ImageReader::open(path)?.with_guessed_format()?;
    let (width, height) = reader.into_dimensions()?;
    ensure!(
        u64::from(width) * u64::from(height) <= 64_000_000,
        "Image dimensions exceed the clipboard limit"
    );
    Ok(())
}
