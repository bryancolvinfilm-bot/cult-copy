use crate::{
    clipboard::Captured,
    model::{Kind, Offer, Part, TextPayload, Version, MAX_PARTS},
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
                "Select between 1 and {MAX_PARTS} files"
            );
            let title = if paths.len() == 1 {
                paths[0]
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned()
            } else {
                format!("{} files", paths.len())
            };
            (Kind::Files, title, paths, None)
        }
    };
    let mut parts = Vec::new();
    let mut total = 0u64;
    let mut names = HashSet::new();
    for path in &source_paths {
        let metadata = fs::metadata(path)?;
        ensure!(
            metadata.is_file(),
            "Folder copying is not supported yet; select the individual files"
        );
        let original =
            crate::model::safe_filename(&path.file_name().unwrap_or_default().to_string_lossy());
        let mut name = original.clone();
        let mut collision = 1;
        while !names.insert(name.to_lowercase()) {
            name = crate::model::safe_filename(&format!("{collision}-{original}"));
            collision += 1;
        }
        let (size, hash) = hash_file(path)?;
        ensure!(
            size == metadata.len(),
            "File changed while preparing it; copy it again"
        );
        total = total.checked_add(size).context("Selection is too large")?;
        parts.push(Part { name, size, hash });
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
    files: Vec<File>,
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
        let mut files = Vec::new();
        for i in 0..offer.parts.len() {
            files.push(
                OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(dir.join(format!("{i}.part")))?,
            );
        }
        let count = files.len();
        Ok(Self {
            transfer: Uuid::new_v4(),
            offer,
            revision,
            explicit_approval: false,
            dir,
            files,
            counts: vec![0; count],
            hashes: vec![blake3::Hasher::new(); count],
            committed: false,
        })
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
        self.files[part].write_all(data)?;
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
            self.files[i].sync_all()?;
        }
        self.files.clear();
        for (i, part) in self.offer.parts.iter().enumerate() {
            fs::rename(
                self.dir.join(format!("{i}.part")),
                self.dir.join(&part.name),
            )?;
        }
        let ready = self
            .dir
            .parent()
            .context("Invalid cache folder")?
            .join(self.offer.id.to_string());
        ensure!(!ready.exists(), "This transfer is already cached");
        fs::rename(&self.dir, &ready)?;
        self.committed = true;
        Ok(self
            .offer
            .parts
            .iter()
            .map(|part| ready.join(&part.name))
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
