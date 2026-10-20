use super::{Snapshot, MAX_INLINE};
use anyhow::{ensure, Context, Result};
use std::path::PathBuf;

const ATTEMPTS: usize = 6;

#[cfg(target_os = "windows")]
pub(super) fn snapshot(clipboard: &super::SystemClipboard) -> Result<Snapshot> {
    use clipboard_rs::{Clipboard, ContentFormat};
    stable_snapshot(
        clipboard_win::seq_num,
        || {
            let snapshot = clipboard.snapshot_once()?;
            if matches!(&snapshot.content, Some(super::Captured::Text(_))) {
                // File metadata can become visible after the initial format
                // checks. Never commit its URL fallback as a text clipboard item.
                ensure!(
                    !clipboard.context.has(ContentFormat::Files)
                        && !clipboard
                            .context
                            .has(ContentFormat::Other("FileNameW".into())),
                    "File selection changed while reading the clipboard"
                );
            }
            Ok(snapshot)
        },
        || std::thread::sleep(std::time::Duration::from_millis(25)),
    )
}

fn stable_snapshot<V: PartialEq>(
    mut version: impl FnMut() -> V,
    mut read: impl FnMut() -> Result<Snapshot>,
    mut wait: impl FnMut(),
) -> Result<Snapshot> {
    let mut last_error = None;
    for attempt in 0..ATTEMPTS {
        let before = version();
        let result = read().and_then(|snapshot| {
            ensure!(before == version(), "Clipboard changed while being read");
            Ok(snapshot)
        });
        match result {
            Ok(snapshot) => return Ok(snapshot),
            Err(error) => last_error = Some(error),
        }
        if attempt + 1 < ATTEMPTS {
            wait();
        }
    }
    Err(last_error.expect("at least one clipboard read"))
        .context("Cannot capture a stable Windows clipboard; try copying again")
}

pub(super) fn filename(bytes: &[u8]) -> Result<PathBuf> {
    ensure!(
        bytes.len() as u64 <= MAX_INLINE && bytes.len().is_multiple_of(2),
        "Invalid Windows file selection"
    );
    let mut units = Vec::new();
    let mut terminated = false;
    for pair in bytes.as_chunks::<2>().0 {
        let unit = u16::from_le_bytes(*pair);
        if unit == 0 {
            terminated = true;
            break;
        }
        units.push(unit);
    }
    ensure!(
        terminated && !units.is_empty(),
        "Invalid Windows file selection"
    );
    Ok(PathBuf::from(String::from_utf16(&units)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{clipboard::Captured, model::TextPayload};
    use std::cell::Cell;

    fn text() -> Snapshot {
        Snapshot {
            content: Some(Captured::Text(TextPayload {
                text: "file:///C:/copied%20file.txt".into(),
                html: None,
                rtf: None,
            })),
            marker: None,
            ignored: false,
        }
    }

    fn files() -> Snapshot {
        Snapshot {
            content: Some(Captured::Files(vec![PathBuf::from(r"C:\copied file.txt")])),
            marker: None,
            ignored: false,
        }
    }

    #[test]
    fn changed_clipboard_retries_instead_of_announcing_a_file_url_as_text() {
        let version = Cell::new(1);
        let reads = Cell::new(0);
        let captured = stable_snapshot(
            || version.get(),
            || {
                reads.set(reads.get() + 1);
                if reads.get() == 1 {
                    version.set(2);
                    Ok(text())
                } else {
                    Ok(files())
                }
            },
            || {},
        )
        .unwrap();
        assert_eq!(reads.get(), 2);
        assert!(matches!(captured.content, Some(Captured::Files(_))));
    }

    #[test]
    fn temporary_clipboard_contention_retries_file_capture() {
        let mut reads = 0;
        let mut waits = 0;
        let captured = stable_snapshot(
            || 1,
            || {
                reads += 1;
                if reads < 3 {
                    anyhow::bail!("OpenClipboard: access denied");
                }
                Ok(files())
            },
            || waits += 1,
        )
        .unwrap();
        assert_eq!((reads, waits), (3, 2));
        assert!(matches!(captured.content, Some(Captured::Files(_))));
    }

    #[test]
    fn unstable_clipboard_never_commits_stale_text_and_stable_url_text_stays_text() {
        let version = Cell::new(0);
        let mut waits = 0;
        assert!(stable_snapshot(
            || version.get(),
            || {
                version.set(version.get() + 1);
                Ok(text())
            },
            || waits += 1
        )
        .is_err());
        assert_eq!(waits, ATTEMPTS - 1);
        let captured = stable_snapshot(|| 1, || Ok(text()), || unreachable!()).unwrap();
        assert!(matches!(captured.content, Some(Captured::Text(_))));
    }

    #[test]
    fn windows_filename_is_a_terminated_unicode_path_not_url_text() {
        let path = r"C:\café folder\copied file.txt";
        let bytes = path
            .encode_utf16()
            .chain([0])
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>();
        assert_eq!(filename(&bytes).unwrap(), PathBuf::from(path));
        for invalid in [&[][..], &[0, 0], &[1], &[1, 0], &[0, 0xd8, 0, 0]] {
            assert!(filename(invalid).is_err());
        }
    }
}
