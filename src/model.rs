use anyhow::{bail, ensure, Result};
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, time::Instant};
use uuid::Uuid;

pub const MIN_LIMIT: u64 = 500_000;
pub const DEFAULT_LIMIT: u64 = 10_000_000;
pub const MAX_LIMIT: u64 = 1_000_000_000;
pub const HISTORY_LIMIT: usize = 30;
pub const MAX_PARTS: usize = 256;
pub const MAX_INLINE: u64 = 64 * 1024 * 1024;
pub const MAX_PATH_DEPTH: usize = 32;
pub const MAX_PART_PATH: usize = 1024;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version {
    pub counter: u64,
    pub origin: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum Kind {
    Text,
    Image,
    Files,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NoticePhase {
    ApprovalNeeded,
    Receiving,
    Ready,
    Saved,
    Declined,
    Cancelled,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClipboardNotice {
    pub event: Uuid,
    pub kind: Kind,
    pub phase: NoticePhase,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Part {
    /// Portable relative path; a trailing slash denotes an empty-payload directory.
    /// Keeping the wire shape unchanged preserves regular-file interoperability.
    pub name: String,
    pub size: u64,
    pub hash: String,
}

impl Part {
    pub fn is_directory(&self) -> bool {
        self.name.ends_with('/')
    }

    pub fn relative_path(&self) -> &str {
        self.name.strip_suffix('/').unwrap_or(&self.name)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Offer {
    pub id: Uuid,
    pub version: Version,
    pub kind: Kind,
    pub title: String,
    pub parts: Vec<Part>,
    pub total: u64,
}

impl Offer {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            !self.parts.is_empty() && self.parts.len() <= MAX_PARTS,
            "Invalid file count"
        );
        ensure!(self.title.len() <= 1024, "Title too long");
        ensure!(self.version.origin.len() == 64, "Invalid device identity");
        let mut names = std::collections::HashSet::new();
        let mut directories = std::collections::HashSet::new();
        let mut total = 0u64;
        for part in &self.parts {
            let path = part.relative_path();
            ensure!(part.name.len() <= MAX_PART_PATH, "File path is too long");
            ensure!(
                path.split('/').count() <= MAX_PATH_DEPTH
                    && path.split('/').all(|name| safe_filename(name) == name),
                "Unsafe file path"
            );
            ensure!(
                self.kind == Kind::Files || (!part.is_directory() && !path.contains('/')),
                "Invalid clipboard path"
            );
            let key = path.to_lowercase();
            if let Some((parent, _)) = path.rsplit_once('/') {
                ensure!(directories.contains(parent), "Missing parent directory");
            }
            ensure!(names.insert(key), "Duplicate file path");
            if part.is_directory() {
                directories.insert(path);
                ensure!(
                    part.size == 0 && part.hash == blake3::hash(b"").to_hex().as_str(),
                    "Invalid directory payload"
                );
            }
            ensure!(
                part.hash.len() == 64 && hex::decode(&part.hash)?.len() == 32,
                "Invalid hash"
            );
            total = total
                .checked_add(part.size)
                .ok_or_else(|| anyhow::anyhow!("Size overflow"))?;
        }
        ensure!(
            total == self.total && total <= 1_000_000_000_000,
            "Invalid transfer size"
        );
        if self.kind != Kind::Files {
            ensure!(
                self.parts.len() == 1 && self.total <= MAX_INLINE,
                "Clipboard item is too large"
            );
        }
        // Offers use a single encrypted control frame, including directory paths.
        use bincode::Options;
        ensure!(
            bincode::DefaultOptions::new()
                .serialized_size(&crate::protocol::Message::Offer(self.clone()))?
                <= (crate::protocol::FRAME_LIMIT - 16) as u64,
            "Selection has too much file metadata"
        );
        Ok(())
    }

    pub fn clipboard_parts(&self) -> impl Iterator<Item = &Part> {
        self.parts
            .iter()
            .filter(|part| !part.relative_path().contains('/'))
    }

    pub fn part_size(&self, part: &Part) -> u64 {
        if part.is_directory() {
            self.parts
                .iter()
                .filter(|child| child.name.starts_with(&part.name))
                .map(|child| child.size)
                .sum()
        } else {
            part.size
        }
    }

    pub fn needs_approval(&self, limit: u64) -> bool {
        self.total > limit.clamp(MIN_LIMIT, MAX_LIMIT)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TextPayload {
    pub text: String,
    pub html: Option<String>,
    pub rtf: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum Status {
    Local,
    AwaitingApproval,
    Receiving,
    Ready,
    Declined,
    Cancelled,
    Failed(String),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HistoryItem {
    pub offer: Offer,
    pub device: String,
    pub peer: String,
    pub status: Status,
    pub paths: Vec<PathBuf>,
    pub preview: Option<String>,
    pub transferred: u64,
    #[serde(skip)]
    pub started: Option<Instant>,
    pub applied: bool,
}

impl HistoryItem {
    pub fn rate(&self) -> Option<f64> {
        let elapsed = self.started?.elapsed().as_secs_f64();
        (elapsed >= 1.0 && self.transferred > 0).then(|| self.transferred as f64 / elapsed)
    }

    pub fn eta(&self) -> Option<f64> {
        self.rate()
            .map(|rate| self.offer.total.saturating_sub(self.transferred) as f64 / rate)
    }
}

pub fn safe_filename(input: &str) -> String {
    let mut name: String = input
        .chars()
        .scan(0usize, |bytes, c| {
            *bytes += c.len_utf8();
            (*bytes <= 170).then_some(c)
        })
        .map(|c| {
            if c.is_control() || "/\\:<>\"|?*".contains(c) {
                '_'
            } else {
                c
            }
        })
        .collect();
    name = name.trim_end_matches([' ', '.']).to_owned();
    let stem = name.split('.').next().unwrap_or_default().to_uppercase();
    if name.is_empty()
        || [
            "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7",
            "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
        ]
        .contains(&stem.as_str())
    {
        name.insert(0, '_');
    }
    name
}

pub fn format_bytes(value: u64) -> String {
    if value >= 1_000_000_000 {
        format!("{:.2} GB", value as f64 / 1e9)
    } else if value >= 1_000_000 {
        format!("{:.1} MB", value as f64 / 1e6)
    } else if value >= 1000 {
        format!("{:.0} KB", value as f64 / 1000.0)
    } else {
        format!("{value} B")
    }
}

pub fn verify_length(actual: u64, expected: u64) -> Result<()> {
    if actual != expected {
        bail!("File size changed during transfer");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    pub fn offer(sizes: &[u64]) -> Offer {
        Offer {
            id: Uuid::new_v4(),
            version: Version {
                counter: 1,
                origin: "a".repeat(64),
            },
            kind: Kind::Files,
            title: "Files".into(),
            total: sizes.iter().sum(),
            parts: sizes
                .iter()
                .enumerate()
                .map(|(i, &size)| Part {
                    name: format!("file{i}"),
                    size,
                    hash: "0".repeat(64),
                })
                .collect(),
        }
    }
    #[test]
    fn combined_size_controls_consent_and_boundary_is_inclusive() {
        assert!(!offer(&[5_000_000, 5_000_000]).needs_approval(DEFAULT_LIMIT));
        assert!(offer(&[6_000_000, 5_000_000]).needs_approval(DEFAULT_LIMIT));
        assert!(offer(&[500_001]).needs_approval(MIN_LIMIT));
        assert!(!offer(&[MAX_LIMIT]).needs_approval(MAX_LIMIT));
    }
    #[test]
    fn reject_paths_duplicate_names_and_forged_sizes() {
        let mut value = offer(&[1, 2]);
        value.validate().unwrap();
        value.parts[0].name = "../escape".into();
        assert!(value.validate().is_err());
        value.parts[0].name = "FILE1".into();
        assert!(value.validate().is_err());
        value.parts[0].name = "okay".into();
        value.total = 100;
        assert!(value.validate().is_err());
        assert_ne!(safe_filename("CON.txt"), "CON.txt");
    }

    #[test]
    fn folder_manifests_require_safe_unique_paths_and_declared_parents() {
        let mut value = offer(&[0, 1]);
        value.parts[0].name = "folder/".into();
        value.parts[0].hash = blake3::hash(b"").to_hex().to_string();
        value.parts[1].name = "folder/file.txt".into();
        value.validate().unwrap();
        for name in [
            "folder/../escape",
            "folder/./escape",
            "folder//escape",
            "/absolute",
            "folder/C:drive",
            "folder/CON.txt",
            "folder\\escape",
            "other/file.txt",
            "FOLDER/file.txt",
            "FOLDER",
        ] {
            value.parts[1].name = name.into();
            assert!(value.validate().is_err(), "Accepted {name}");
        }
        value.parts[1].name = "folder/file.txt".into();
        value.parts[0].name = "folder".into();
        assert!(value.validate().is_err(), "A file cannot be a parent");
        value.parts[0].name = "folder/".into();
        value.parts[0].size = 1;
        value.total = 2;
        assert!(value.validate().is_err(), "Directories cannot carry bytes");
        value.parts[0].size = 0;
        value.total = 1;
        value.kind = Kind::Image;
        assert!(value.validate().is_err());
    }

    #[test]
    fn metadata_must_fit_in_one_control_frame() {
        let mut value = offer(&[0; MAX_PARTS]);
        for (i, part) in value.parts.iter_mut().enumerate() {
            part.name = format!("{i}-{}", "x".repeat(160));
        }
        value.validate().unwrap();
        // Deep directory paths also count against the frame limit.
        let mut directory = value.parts[0].clone();
        directory.name = format!("{}/", "d".repeat(160));
        directory.hash = blake3::hash(b"").to_hex().to_string();
        for part in &mut value.parts[1..] {
            part.name = format!("{}{}", directory.name, part.name);
        }
        value.parts[0] = directory;
        assert!(value.validate().is_err());
    }
}
