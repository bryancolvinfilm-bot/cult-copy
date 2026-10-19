use crate::{
    clipboard::Captured,
    model::{Kind, Offer, Part, TextPayload, Version, MAX_PARTS, MAX_PART_PATH, MAX_PATH_DEPTH},
    protocol::CHUNK_SIZE,
};
use anyhow::{ensure, Context, Result};
use std::{
    collections::HashSet,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct Outgoing {
    pub offer: Offer,
    pub paths: Vec<PathBuf>,
    pub preview: Option<String>,
}

impl Outgoing {
    /// History and the clipboard retain only the selected roots. Resolve the
    /// manifest's nested entries against those roots when streaming a transfer.
    pub fn part_paths(&self) -> Result<Vec<PathBuf>> {
        self.offer.validate()?;
        let roots: Vec<_> = self.offer.clipboard_parts().collect();
        ensure!(roots.len() == self.paths.len(), "Missing source paths");
        let roots: std::collections::HashMap<_, _> = roots
            .into_iter()
            .zip(&self.paths)
            .map(|(part, path)| (part.relative_path(), path))
            .collect();
        let mut resolved: std::collections::HashMap<String, PathBuf> =
            std::collections::HashMap::new();
        let mut paths = Vec::new();
        for part in &self.offer.parts {
            let relative = part.relative_path();
            let path = if relative.contains('/') {
                resolved
                    .get(relative)
                    .context("Source folder contents changed; copy it again")?
                    .clone()
            } else {
                roots
                    .get(relative)
                    .context("Missing source root")?
                    .to_path_buf()
            };
            let metadata = source_metadata(&path)?;
            ensure!(
                metadata.is_dir() == part.is_directory(),
                "Source type changed; copy it again"
            );
            if part.is_directory() {
                for (name, child) in named_children(&path)? {
                    resolved.insert(format!("{relative}/{name}"), child);
                }
            }
            paths.push(path);
        }
        Ok(paths)
    }
}

fn unique_name(path: &Path, names: &mut HashSet<String>) -> String {
    let original =
        crate::model::safe_filename(&path.file_name().unwrap_or_default().to_string_lossy());
    let mut name = original.clone();
    let mut collision = 1;
    while !names.insert(name.to_lowercase()) {
        name = crate::model::safe_filename(&format!("{collision}-{original}"));
        collision += 1;
    }
    name
}

fn named_children(path: &Path) -> Result<Vec<(String, PathBuf)>> {
    let mut children = fs::read_dir(path)?
        .take(MAX_PARTS + 1)
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    ensure!(
        children.len() <= MAX_PARTS,
        "Too many files and folders in the selection"
    );
    children.sort();
    let mut names = HashSet::new();
    Ok(children
        .into_iter()
        .map(|path| (unique_name(&path, &mut names), path))
        .collect())
}

fn source_metadata(path: &Path) -> Result<fs::Metadata> {
    let metadata =
        fs::symlink_metadata(path).with_context(|| format!("Cannot read {}", path.display()))?;
    ensure!(
        !metadata.is_symlink(),
        "Symbolic links are not supported: {}",
        path.display()
    );
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::fs::MetadataExt;
        ensure!(
            metadata.file_attributes() & 0x400 == 0,
            "Junctions and other reparse points are not supported: {}",
            path.display()
        );
    }
    ensure!(
        metadata.is_file() || metadata.is_dir(),
        "Only regular files and folders can be copied: {}",
        path.display()
    );
    Ok(metadata)
}

fn append_path(path: &Path, name: String, parts: &mut Vec<Part>, total: &mut u64) -> Result<()> {
    ensure!(
        parts.len() < MAX_PARTS,
        "A selection can contain at most {MAX_PARTS} files and folders, including nested entries"
    );
    ensure!(
        name.split('/').count() <= MAX_PATH_DEPTH && name.len() < MAX_PART_PATH,
        "Folder structure is too deep or its paths are too long"
    );
    let metadata = source_metadata(path)?;
    if metadata.is_dir() {
        parts.push(Part {
            name: format!("{name}/"),
            size: 0,
            hash: blake3::hash(b"").to_hex().to_string(),
        });
        let children = named_children(path)?;
        ensure!(
            children.len() <= MAX_PARTS - parts.len(),
            "A selection can contain at most {MAX_PARTS} files and folders, including nested entries"
        );
        for (child_name, child) in children {
            append_path(&child, format!("{name}/{child_name}"), parts, total)?;
        }
    } else {
        let (size, hash) = hash_file(path)?;
        ensure!(
            size == metadata.len(),
            "File changed while preparing it; copy it again"
        );
        *total = total.checked_add(size).context("Selection is too large")?;
        parts.push(Part { name, size, hash });
    }
    Ok(())
}

pub fn hash_file(path: &Path) -> Result<(u64, String)> {
    let mut file = File::open(path).with_context(|| format!("Cannot read {}", path.display()))?;
    let mut hash = blake3::Hasher::new();
    let mut buffer = vec![0u8; CHUNK_SIZE];
    let mut count = 0u64;
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
        count += n as u64;
    }
    Ok((count, hash.finalize().to_hex().to_string()))
}

pub fn prepare(captured: Captured, version: Version, root: &Path) -> Result<Outgoing> {
    let id = Uuid::new_v4();
    let dir = root.join(id.to_string());
    fs::create_dir_all(&dir)?;
    let (kind, title, source_paths, preview) = match captured {
        Captured::Text(payload) => {
            let title = payload
                .text
                .lines()
                .next()
                .unwrap_or("Text")
                .chars()
                .take(100)
                .collect::<String>();
            let preview = Some(payload.text.chars().take(10_000).collect());
            let path = dir.join("text.json");
            fs::write(&path, serde_json::to_vec(&payload)?)?;
            (Kind::Text, title, vec![path], preview)
        }
        Captured::Image(bytes) => {
            let path = dir.join("image.png");
            fs::write(&path, bytes)?;
            (Kind::Image, "Copied image".to_string(), vec![path], None)
        }
        Captured::Files(paths) => {
            ensure!(
                !paths.is_empty() && paths.len() <= MAX_PARTS,
                "Select between 1 and {MAX_PARTS} files or folders"
            );
            let title = if paths.len() == 1 {
                paths[0]
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned()
            } else {
                format!("{} files or folders", paths.len())
            };
            (Kind::Files, title, paths, None)
        }
    };
    let mut parts = Vec::new();
    let mut total = 0u64;
    let mut names = HashSet::new();
    for path in &source_paths {
        let name = unique_name(path, &mut names);
        append_path(path, name, &mut parts, &mut total)?;
    }
    let offer = Offer {
        id,
        version,
        kind,
        title: title.chars().take(180).collect(),
        parts,
        total,
    };
    offer.validate()?;
    Ok(Outgoing {
        offer,
        paths: source_paths,
        preview,
    })
}

pub struct Receiver {
    pub transfer: Uuid,
    pub offer: Offer,
    pub revision: u64,
    pub explicit_approval: bool,
    dir: PathBuf,
    files: Vec<Option<File>>,
    counts: Vec<u64>,
    hashes: Vec<blake3::Hasher>,
    committed: bool,
}

impl Receiver {
    pub fn new(offer: Offer, revision: u64, root: &Path) -> Result<Self> {
        offer.validate()?;
        fs::create_dir_all(root)?;
        if let Ok(available) = fs2::available_space(root) {
            ensure!(
                offer.total <= available.saturating_sub(16_000_000),
                "Not enough disk space for this copy"
            );
        }
        let dir = root.join(format!("{}.partial", offer.id));
        // A random suffix also makes a retry independent of abandoned temporary files.
        let dir = dir.with_extension(format!("{}.partial", Uuid::new_v4()));
        fs::create_dir(&dir)?;
        let count = offer.parts.len();
        let mut receiver = Self {
            transfer: Uuid::new_v4(),
            offer,
            revision,
            explicit_approval: false,
            dir,
            files: Vec::new(),
            counts: vec![0; count],
            hashes: vec![blake3::Hasher::new(); count],
            committed: false,
        };
        for (i, part) in receiver.offer.parts.iter().enumerate() {
            receiver.files.push(if part.is_directory() {
                None
            } else {
                Some(
                    OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(receiver.dir.join(format!("{i}.part")))?,
                )
            });
        }
        Ok(receiver)
    }

    pub fn write(&mut self, part: usize, offset: u64, data: &[u8]) -> Result<()> {
        let expected = self.offer.parts.get(part).context("Unknown file")?;
        ensure!(
            data.len() <= CHUNK_SIZE && !data.is_empty(),
            "Invalid transfer chunk"
        );
        ensure!(offset == self.counts[part], "Out-of-order chunk");
        let next = offset
            .checked_add(data.len() as u64)
            .context("Size overflow")?;
        ensure!(next <= expected.size, "File exceeds its announced size");
        self.files[part]
            .as_mut()
            .context("Directories cannot receive file data")?
            .write_all(data)?;
        self.hashes[part].update(data);
        self.counts[part] = next;
        Ok(())
    }

    pub fn transferred(&self) -> u64 {
        self.counts.iter().sum()
    }

    pub fn finish(mut self) -> Result<Vec<PathBuf>> {
        for (i, part) in self.offer.parts.iter().enumerate() {
            ensure!(self.counts[i] == part.size, "Transfer is incomplete");
            ensure!(
                self.hashes[i].finalize().to_hex().as_str() == part.hash,
                "File integrity check failed"
            );
            if let Some(file) = &self.files[i] {
                file.sync_all()?;
            }
        }
        self.files.clear();
        // Assemble in a separate tree so user names such as `0.part` cannot
        // collide with the numbered staging files. Nothing is published early.
        let tree = self.dir.join("tree");
        fs::create_dir(&tree)?;
        for (i, part) in self.offer.parts.iter().enumerate() {
            let destination = tree.join(part.relative_path());
            if part.is_directory() {
                fs::create_dir(&destination)?;
            } else {
                fs::rename(self.dir.join(format!("{i}.part")), destination)?;
            }
        }
        let ready = self
            .dir
            .parent()
            .context("Invalid cache folder")?
            .join(self.offer.id.to_string());
        ensure!(!ready.exists(), "This transfer is already cached");
        fs::rename(&tree, &ready)?;
        self.committed = true;
        let _ = fs::remove_dir(&self.dir);
        Ok(self
            .offer
            .clipboard_parts()
            .map(|part| ready.join(part.relative_path()))
            .collect())
    }
}

impl Drop for Receiver {
    fn drop(&mut self) {
        self.files.clear();
        if !self.committed {
            let _ = fs::remove_dir_all(&self.dir);
        }
    }
}

pub fn text_preview(path: &Path) -> Result<TextPayload> {
    ensure!(
        fs::metadata(path)?.len() <= crate::model::MAX_INLINE,
        "Text is too large"
    );
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn prepare_files(paths: Vec<PathBuf>, root: &Path) -> Outgoing {
        prepare(
            Captured::Files(paths),
            Version {
                counter: 1,
                origin: "a".repeat(64),
            },
            root,
        )
        .unwrap()
    }

    fn transfer(item: &Outgoing, root: &Path) -> Vec<PathBuf> {
        let mut job = Receiver::new(item.offer.clone(), 1, root).unwrap();
        for (i, path) in item.part_paths().unwrap().iter().enumerate() {
            if item.offer.parts[i].is_directory() {
                continue;
            }
            let bytes = fs::read(path).unwrap();
            for (chunk, data) in bytes.chunks(CHUNK_SIZE).enumerate() {
                job.write(i, (chunk * CHUNK_SIZE) as u64, data).unwrap();
            }
        }
        assert_eq!(job.transferred(), item.offer.total);
        job.finish().unwrap()
    }

    #[test]
    fn folders_round_trip_with_contents_empty_directories_and_selected_roots() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("Project café");
        fs::create_dir_all(folder.join("nested/empty")).unwrap();
        fs::write(folder.join("nested/file.txt"), b"nested bytes").unwrap();
        fs::write(folder.join(".hidden"), b"hidden").unwrap();
        fs::write(folder.join("empty.txt"), b"").unwrap();
        // These names must not collide with the transfer's staging files/tree.
        let file = dir.path().join("0.part");
        fs::write(&file, b"root bytes").unwrap();
        let item = prepare_files(
            vec![folder.clone(), file.clone()],
            &dir.path().join("outgoing"),
        );
        assert_eq!(item.paths, [folder.clone(), file]);
        assert_eq!(item.offer.total, 28);
        let paths = transfer(&item, &dir.path().join("received"));
        assert_eq!(paths.len(), 2);
        assert_eq!(paths[0].file_name().unwrap(), "Project café");
        assert!(paths[0].join("nested/empty").is_dir());
        assert_eq!(
            fs::read(paths[0].join("nested/file.txt")).unwrap(),
            b"nested bytes"
        );
        assert_eq!(fs::read(paths[0].join(".hidden")).unwrap(), b"hidden");
        assert_eq!(fs::read(paths[0].join("empty.txt")).unwrap(), b"");
        assert_eq!(fs::read(&paths[1]).unwrap(), b"root bytes");
        assert!(folder.join("nested/file.txt").is_file());
        // A received selection remains copyable and transferable from history.
        let copied = prepare_files(paths, &dir.path().join("recopy"));
        let again = transfer(&copied, &dir.path().join("again"));
        assert_eq!(
            fs::read(again[0].join("nested/file.txt")).unwrap(),
            b"nested bytes"
        );
    }

    #[test]
    fn empty_folder_completes_without_chunks_and_cancelled_folder_leaves_no_cache() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("tree");
        fs::create_dir(&folder).unwrap();
        let item = prepare_files(vec![folder], &dir.path().join("outgoing"));
        let paths = transfer(&item, &dir.path().join("received"));
        assert!(paths[0].is_dir());
        assert_eq!(fs::read_dir(&paths[0]).unwrap().count(), 0);
        let mut job = Receiver::new(item.offer, 1, &dir.path().join("cancelled")).unwrap();
        assert!(job.write(0, 0, b"invalid directory bytes").is_err());
        let temporary = job.dir.clone();
        drop(job);
        assert!(!temporary.exists());
    }

    #[cfg(unix)]
    #[test]
    fn portable_name_collisions_resolve_to_the_correct_original_files() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("folder");
        fs::create_dir(&folder).unwrap();
        for (name, bytes) in [("a:b", b"one"), ("a?b", b"two"), ("A_B", b"tri")] {
            fs::write(folder.join(name), bytes).unwrap();
        }
        let item = prepare_files(vec![folder], &dir.path().join("outgoing"));
        let paths = transfer(&item, &dir.path().join("received"));
        assert_eq!(fs::read(paths[0].join("A_B")).unwrap(), b"tri");
        assert_eq!(fs::read(paths[0].join("1-a_b")).unwrap(), b"one");
        assert_eq!(fs::read(paths[0].join("2-a_b")).unwrap(), b"two");
    }

    #[cfg(unix)]
    #[test]
    fn symbolic_links_are_rejected_instead_of_followed() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("folder");
        fs::create_dir(&folder).unwrap();
        std::os::unix::fs::symlink(&folder, folder.join("loop")).unwrap();
        let result = prepare(
            Captured::Files(vec![folder]),
            Version {
                counter: 1,
                origin: "a".repeat(64),
            },
            &dir.path().join("outgoing"),
        );
        assert!(result.unwrap_err().to_string().contains("Symbolic links"));
    }

    #[test]
    fn too_many_nested_entries_fail_before_announcing_a_partial_selection() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("folder");
        fs::create_dir(&folder).unwrap();
        for i in 0..MAX_PARTS {
            fs::write(folder.join(format!("file{i}")), b"").unwrap();
        }
        assert!(prepare(
            Captured::Files(vec![folder]),
            Version {
                counter: 1,
                origin: "a".repeat(64)
            },
            &dir.path().join("outgoing")
        )
        .is_err());
    }

    #[test]
    fn filenames_that_match_other_staged_parts_preserve_every_file() {
        let dir = tempfile::tempdir().unwrap();
        let one = dir.path().join("1.part");
        let two = dir.path().join("0.part");
        fs::write(&one, b"first").unwrap();
        fs::write(&two, b"second").unwrap();
        let item = prepare_files(vec![one, two], &dir.path().join("outgoing"));
        let paths = transfer(&item, &dir.path().join("received"));
        assert_eq!(fs::read(&paths[0]).unwrap(), b"first");
        assert_eq!(fs::read(&paths[1]).unwrap(), b"second");
    }
    fn setup(bytes: &[u8]) -> (tempfile::TempDir, Offer) {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("original.txt");
        fs::write(&file, bytes).unwrap();
        let item = prepare(
            Captured::Files(vec![file]),
            Version {
                counter: 1,
                origin: "a".repeat(64),
            },
            &dir.path().join("outgoing"),
        )
        .unwrap();
        (dir, item.offer)
    }
    #[test]
    fn commit_requires_complete_verified_bytes_and_preserves_filename() {
        let (dir, offer) = setup(b"actual bytes");
        let mut job = Receiver::new(offer.clone(), 1, &dir.path().join("received")).unwrap();
        job.write(0, 0, b"actual bytes").unwrap();
        let paths = job.finish().unwrap();
        assert_eq!(paths[0].file_name().unwrap(), "original.txt");
        assert_eq!(fs::read(&paths[0]).unwrap(), b"actual bytes");
        let mut job = Receiver::new(offer, 1, &dir.path().join("corrupt")).unwrap();
        job.write(0, 0, b"wrong! bytes").unwrap();
        assert!(job.finish().is_err());
    }
    #[test]
    fn rejects_incomplete_oversized_and_out_of_order_data_and_cleans_up() {
        let (dir, offer) = setup(b"hello");
        let mut job = Receiver::new(offer.clone(), 1, &dir.path().join("received")).unwrap();
        assert!(job.write(0, 1, b"h").is_err());
        assert!(job.write(0, 0, b"too long").is_err());
        let temporary = job.dir.clone();
        assert!(job.finish().is_err());
        assert!(!temporary.exists());
    }
}
