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

#[cfg(any(target_os = "windows", test))]
mod windows;

#[cfg(target_os = "macos")]
pub const MARKER: &str = "io.duetclipboard.clipboard-event";
#[cfg(not(target_os = "macos"))]
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
            // A change inside a folder does not necessarily change the root's
            // timestamp. Include nested metadata so copying it again is fresh.
            // Use the same bounds as a transfer and never follow symbolic links.
            let mut pending: Vec<_> = paths.iter().map(|path| (path.clone(), 1)).collect();
            for _ in 0..=crate::model::MAX_PARTS {
                let Some((path, depth)) = pending.pop() else {
                    break;
                };
                hash.update(&bincode::serialize(&path).expect("serializable path"));
                if let Ok(metadata) = std::fs::symlink_metadata(&path) {
                    hash.update(&[u8::from(metadata.is_dir()), u8::from(metadata.is_symlink())]);
                    hash.update(&metadata.len().to_le_bytes());
                    if let Ok(modified) = metadata.modified().and_then(|time| {
                        time.duration_since(std::time::UNIX_EPOCH)
                            .map_err(std::io::Error::other)
                    }) {
                        hash.update(&modified.as_nanos().to_le_bytes());
                    }
                    if metadata.is_dir() && depth < crate::model::MAX_PATH_DEPTH {
                        if let Ok(entries) = std::fs::read_dir(&path) {
                            let mut children: Vec<_> = entries
                                .take(crate::model::MAX_PARTS + 1)
                                .filter_map(|entry| entry.ok().map(|entry| entry.path()))
                                .collect();
                            children.sort();
                            pending
                                .extend(children.into_iter().rev().map(|path| (path, depth + 1)));
                        }
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
    /// An external sync announcement must not invalidate a local copy/transfer.
    pub ignored: bool,
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
        #[cfg(target_os = "windows")]
        return windows::snapshot(self);
        #[cfg(not(target_os = "windows"))]
        self.snapshot_once()
    }

    fn snapshot_once(&self) -> Result<Snapshot> {
        let formats = self
            .context
            .available_formats()
            .map_err(|e| anyhow::anyhow!("Cannot read clipboard: {e}"))?;
        // Inspect types only: reading a promised remote value (even our marker)
        // can ask Universal Clipboard to fetch it and display Apple's progress UI.
        #[cfg(target_os = "macos")]
        if formats.iter().any(|s| s == "com.apple.is-remote-clipboard") {
            return Ok(Snapshot {
                content: None,
                marker: None,
                ignored: true,
            });
        }
        let marker = formats
            .iter()
            .any(|format| format == MARKER)
            .then(|| {
                self.context
                    .get_buffer(MARKER)
                    .ok()
                    .and_then(|data| Uuid::from_slice(&data).ok())
            })
            .flatten();
        // Respect the common Windows, Apple, and KDE sensitive-content conventions.
        let sensitive = formats.iter().any(|s| {
            matches!(
                s.as_str(),
                "ExcludeClipboardContentFromMonitorProcessing"
                    | "org.nspasteboard.ConcealedType"
                    | "org.nspasteboard.TransientType"
            )
        });
        let kde_sensitive = formats
            .iter()
            .any(|format| format == "x-kde-passwordManagerHint")
            && self
                .context
                .get_buffer("x-kde-passwordManagerHint")
                .ok()
                .is_some_and(|data| data == b"secret");
        if sensitive || kde_sensitive {
            return Ok(Snapshot {
                content: None,
                marker,
                ignored: false,
            });
        }
        let uri_files = || {
            FILE_LIST_FORMATS
                .iter()
                .find(|format| formats.iter().any(|offered| offered == **format))
                .map(|format| {
                    let bytes = self
                        .context
                        .get_buffer(format)
                        .map_err(|e| anyhow::anyhow!("Cannot read copied files: {e}"))?;
                    file_list_paths(format, &bytes)
                })
                .transpose()
        };
        #[cfg(target_os = "linux")]
        let files = uri_files()?;
        #[cfg(not(target_os = "linux"))]
        let files = if self.context.has(ContentFormat::Files) {
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
            Some(paths)
        } else {
            #[cfg(target_os = "windows")]
            let filename = if formats.iter().any(|format| format == "FileNameW") {
                let bytes = self
                    .context
                    .get_buffer("FileNameW")
                    .map_err(|e| anyhow::anyhow!("Cannot read copied file: {e}"))?;
                let path = windows::filename(&bytes)?;
                ensure!(path.is_absolute(), "Invalid copied file path");
                Some(vec![path])
            } else {
                None
            };
            #[cfg(not(target_os = "windows"))]
            let filename: Option<Vec<PathBuf>> = None;
            if filename.is_some() {
                filename
            } else {
                uri_files()?
            }
        };
        let content = if let Some(paths) = files {
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
            if text.trim_start().starts_with("file:") {
                // Diagnose lost file formats without logging clipboard paths or
                // contents. Plain URL text alone must not authorize file uploads.
                tracing::warn!(formats = ?formats,
                    "Clipboard contains file URL text without a supported file selection");
            }
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
        Ok(Snapshot {
            content,
            marker,
            ignored: false,
        })
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
                #[cfg(target_os = "linux")]
                let values = linux_file_contents(paths)?;
                #[cfg(not(target_os = "linux"))]
                let values = vec![ClipboardContent::Files(
                    paths
                        .iter()
                        .map(|path| path.to_string_lossy().into_owned())
                        .collect(),
                )];
                values
            }
        };
        values.push(ClipboardContent::Other(
            MARKER.into(),
            id.as_bytes().to_vec(),
        ));
        #[cfg(target_os = "macos")]
        macos_write(values)?;
        #[cfg(not(target_os = "macos"))]
        self.context
            .set(values)
            .map_err(|e| anyhow::anyhow!("Cannot update clipboard: {e}"))?;
        Ok(())
    }
}

const FILE_LIST_FORMATS: [&str; 3] = [
    "text/uri-list",
    "x-special/gnome-copied-files",
    "x-special/nautilus-clipboard",
];

fn file_list_paths(format: &str, bytes: &[u8]) -> Result<Vec<PathBuf>> {
    ensure!(
        bytes.len() as u64 <= MAX_INLINE,
        "File selection is too large"
    );
    let mut lines = std::str::from_utf8(bytes)?.trim_end_matches('\0').lines();
    if format != "text/uri-list" {
        let mut action = lines.next();
        if action == Some("x-special/nautilus-clipboard") {
            action = lines.next();
        }
        ensure!(
            matches!(action, Some("copy" | "cut")),
            "Invalid file clipboard action"
        );
    }
    let paths = lines
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| {
            url::Url::parse(line)?
                .to_file_path()
                .map_err(|_| anyhow::anyhow!("Invalid local file URL"))
        })
        .collect::<Result<Vec<_>>>()?;
    ensure!(
        paths.len() <= crate::model::MAX_PARTS,
        "Too many copied files"
    );
    Ok(paths)
}

#[cfg(target_os = "linux")]
fn linux_file_contents(paths: &[PathBuf]) -> Result<Vec<ClipboardContent>> {
    let files = paths
        .iter()
        .map(|path| {
            url::Url::from_file_path(path)
                .map(|url| url.to_string())
                .map_err(|_| anyhow::anyhow!("Invalid local file path"))
        })
        .collect::<Result<Vec<_>>>()?;
    let copied = format!("copy\n{}", files.join("\n")).into_bytes();
    // The X11 Files adapter also advertises plain text paths. Some paste targets
    // choose that text instead of attaching the files. Publish file formats only.
    Ok(vec![
        ClipboardContent::Other(
            "text/uri-list".into(),
            format!("{}\r\n", files.join("\r\n")).into_bytes(),
        ),
        ClipboardContent::Other("x-special/gnome-copied-files".into(), copied.clone()),
        ClipboardContent::Other("x-special/nautilus-clipboard".into(), copied),
        ClipboardContent::Other("application/x-kde-cutselection".into(), b"0".to_vec()),
    ])
}

#[cfg(target_os = "macos")]
fn macos_write(values: Vec<ClipboardContent>) -> Result<()> {
    use objc2::rc::autoreleasepool;
    use objc2_app_kit::NSPasteboardItem;
    use objc2_foundation::NSString;

    autoreleasepool(|_| {
        let item = NSPasteboardItem::new();
        for value in values {
            let written = match value {
                ClipboardContent::Text(text) => item.setString_forType(
                    &NSString::from_str(&text),
                    &NSString::from_str("public.utf8-plain-text"),
                ),
                ClipboardContent::Html(html) => item.setString_forType(
                    &NSString::from_str(&html),
                    &NSString::from_str("public.html"),
                ),
                ClipboardContent::Rtf(rtf) => macos_set_data(&item, "public.rtf", rtf.as_bytes()),
                ClipboardContent::Image(image) => {
                    let png = image
                        .to_png()
                        .map_err(|e| anyhow::anyhow!("Cannot encode clipboard image: {e}"))?;
                    macos_set_data(&item, "public.png", png.get_bytes())
                }
                ClipboardContent::Other(format, data) => macos_set_data(&item, &format, &data),
                ClipboardContent::Files(_) => {
                    anyhow::bail!("Files need individual pasteboard items")
                }
            };
            ensure!(written, "Cannot prepare clipboard content");
        }
        macos_publish(&[item])
    })
}

#[cfg(target_os = "macos")]
fn macos_set_data(item: &objc2_app_kit::NSPasteboardItem, format: &str, bytes: &[u8]) -> bool {
    // NSData copies the bytes before this stack frame returns.
    let data = unsafe {
        objc2_foundation::NSData::dataWithBytes_length(bytes.as_ptr().cast(), bytes.len())
    };
    item.setData_forType(&data, &objc2_foundation::NSString::from_str(format))
}

#[cfg(target_os = "macos")]
fn macos_publish(items: &[objc2::rc::Retained<objc2_app_kit::NSPasteboardItem>]) -> Result<()> {
    use objc2::runtime::ProtocolObject;
    use objc2_app_kit::{NSPasteboard, NSPasteboardContentsOptions, NSPasteboardWriting};
    use objc2_foundation::NSArray;

    let objects: Vec<_> = items
        .iter()
        .map(|item| ProtocolObject::<dyn NSPasteboardWriting>::from_retained(item.clone()))
        .collect();
    let pasteboard = NSPasteboard::generalPasteboard();
    // This must replace clearContents(), not precede it: clearing again would
    // discard CurrentHostOnly. Cult Copy already delivered these items over its LAN.
    pasteboard.prepareForNewContentsWithOptions(NSPasteboardContentsOptions::CurrentHostOnly);
    ensure!(
        pasteboard.writeObjects(&NSArray::from_retained_slice(&objects)),
        "Cannot update clipboard"
    );
    Ok(())
}

#[cfg(target_os = "macos")]
fn macos_files(paths: &[PathBuf], id: Uuid) -> Result<()> {
    use objc2::rc::autoreleasepool;
    use objc2_app_kit::NSPasteboardItem;
    use objc2_foundation::NSString;
    autoreleasepool(|_| {
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
            ensure!(
                macos_set_data(&item, MARKER, id.as_bytes()),
                "Cannot prepare file clipboard marker"
            );
            items.push(item);
        }
        macos_publish(&items)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_file_formats_preserve_paths_and_do_not_publish_url_text() {
        let paths = vec![
            PathBuf::from("/tmp/café #1.txt"),
            PathBuf::from("/tmp/a\nb.txt"),
        ];
        let contents = linux_file_contents(&paths).unwrap();
        for format in FILE_LIST_FORMATS {
            let bytes = contents
                .iter()
                .find_map(|content| match content {
                    ClipboardContent::Other(name, bytes) if name == format => Some(bytes),
                    _ => None,
                })
                .unwrap();
            assert_eq!(file_list_paths(format, bytes).unwrap(), paths);
        }
        assert!(contents.iter().all(|content| matches!(content,
            ClipboardContent::Other(format, _) if !format.starts_with("text/plain"))));
        assert_eq!(
            file_list_paths(
                "x-special/nautilus-clipboard",
                b"x-special/nautilus-clipboard\ncut\nfile:///tmp/a%20b\n"
            )
            .unwrap(),
            vec![PathBuf::from("/tmp/a b")]
        );
        assert_eq!(
            file_list_paths("text/uri-list", b"# comment\r\nfile:///tmp/a\r\n").unwrap(),
            vec![PathBuf::from("/tmp/a")]
        );
        assert_eq!(
            file_list_paths("text/uri-list", b"file:///tmp/a\r\n\0\0").unwrap(),
            vec![PathBuf::from("/tmp/a")]
        );
        for bytes in [
            b"https://example.com/file".as_slice(),
            b"file://other-computer/tmp/a",
            b"/tmp/a",
        ] {
            assert!(file_list_paths("text/uri-list", bytes).is_err());
        }
        assert!(file_list_paths("x-special/gnome-copied-files", b"file:///tmp/a").is_err());
    }

    #[test]
    fn copying_a_folder_again_detects_changes_to_nested_files() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("folder");
        std::fs::create_dir_all(folder.join("nested")).unwrap();
        let path = folder.join("nested/file.txt");
        std::fs::write(&path, b"before").unwrap();
        let captured = Captured::Files(vec![folder]);
        let before = captured.signature();
        assert_eq!(before, captured.signature());
        std::fs::write(&path, b"after with different length").unwrap();
        let after = captured.signature();
        assert_ne!(before, after);
        assert_eq!(after, captured.signature());
    }
}
