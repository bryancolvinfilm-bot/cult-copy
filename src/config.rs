use crate::model::{DEFAULT_LIMIT, MAX_LIMIT, MIN_LIMIT};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub name: String,
    pub private_key: String,
    pub public_key: String,
    pub auto_receive_limit: u64,
    pub ask_before_receiving: bool,
    pub paused: bool,
    pub start_at_login: bool,
    pub clock: u64,
    pub trusted: BTreeMap<String, String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            name: hostname::get()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            private_key: String::new(),
            public_key: String::new(),
            auto_receive_limit: DEFAULT_LIMIT,
            ask_before_receiving: false,
            paused: false,
            start_at_login: false,
            clock: 0,
            trusted: BTreeMap::new(),
        }
    }
}

impl Config {
    pub fn load(dir: &Path) -> Result<Self> {
        fs::create_dir_all(dir)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(dir, fs::Permissions::from_mode(0o700))?;
        }
        let path = dir.join("settings.json");
        let mut config: Self = if path.exists() {
            serde_json::from_slice(&fs::read(path)?)
                .context("Cannot read settings; the original file has been preserved")?
        } else {
            Self::default()
        };
        if config.private_key.is_empty() {
            let keys = snow::Builder::new(crate::protocol::noise_params()).generate_keypair()?;
            config.private_key = hex::encode(keys.private);
            config.public_key = hex::encode(keys.public);
        }
        anyhow::ensure!(
            hex::decode(&config.private_key)?.len() == 32
                && hex::decode(&config.public_key)?.len() == 32,
            "Invalid device key"
        );
        config.auto_receive_limit = config.auto_receive_limit.clamp(MIN_LIMIT, MAX_LIMIT);
        config.save(dir)?;
        Ok(config)
    }

    pub fn id(&self) -> String {
        blake3::hash(&hex::decode(&self.public_key).expect("validated key"))
            .to_hex()
            .to_string()
    }

    pub fn save(&self, dir: &Path) -> Result<()> {
        let temporary = dir.join("settings.tmp");
        fs::write(&temporary, serde_json::to_vec_pretty(self)?)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&temporary, fs::Permissions::from_mode(0o600))?;
        }
        fs::rename(temporary, dir.join("settings.json"))?;
        Ok(())
    }
}

pub fn default_dir() -> Result<PathBuf> {
    #[cfg(target_os = "linux")]
    let application = "CultCopy";
    #[cfg(not(target_os = "linux"))]
    let application = "DuetClipboard";
    Ok(
        directories::ProjectDirs::from("io", application, application)
            .context("Cannot locate your application data folder")?
            .data_local_dir()
            .to_owned(),
    )
}

pub fn prepare_default_dir() -> Result<PathBuf> {
    let destination = default_dir()?;
    #[cfg(target_os = "linux")]
    {
        let legacy = directories::ProjectDirs::from("io", "DuetClipboard", "DuetClipboard")
            .context("Cannot locate the previous application data folder")?;
        migrate_profile(legacy.data_local_dir(), &destination)?;
        let base = directories::BaseDirs::new().context("Cannot locate home folder")?;
        remove_legacy_launchers(base.data_local_dir(), base.config_dir())?;
    }
    Ok(destination)
}

#[cfg(target_os = "linux")]
fn remove_legacy_launchers(data: &Path, config: &Path) -> Result<()> {
    // A per-user launcher overrides package upgrades and can keep opening an
    // older executable. The new package and login entry use the canonical ID.
    for path in [
        data.join("applications/duet-clipboard.desktop"),
        data.join("icons/hicolor/256x256/apps/duet-clipboard.png"),
        data.join("icons/hicolor/scalable/apps/duet-clipboard.svg"),
        config.join("autostart/duet-clipboard.desktop"),
    ] {
        if path.is_file() || path.is_symlink() {
            fs::remove_file(path)?;
        }
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn migrate_profile(legacy: &Path, destination: &Path) -> Result<()> {
    let temporary = "history.migration.tmp";
    // Recover if the previous startup stopped after moving the directory but
    // before atomically replacing the rebased history file.
    if destination.join(temporary).is_file() {
        let lock = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(destination.join("instance.lock"))?;
        fs2::FileExt::try_lock_exclusive(&lock)
            .context("Quit the running Cult Copy instance before completing migration")?;
        fs::rename(
            destination.join(temporary),
            destination.join("history.json"),
        )?;
    }
    if !legacy.join("settings.json").is_file() || destination.join("settings.json").exists() {
        return Ok(());
    }
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(legacy.join("instance.lock"))?;
    fs2::FileExt::try_lock_exclusive(&lock)
        .context("Quit the previous Cult Copy instance before upgrading")?;
    let history_path = legacy.join("history.json");
    let history = if history_path.is_file() {
        let mut history: Vec<serde_json::Value> =
            serde_json::from_slice(&fs::read(&history_path)?)?;
        for item in &mut history {
            if let Some(paths) = item
                .get_mut("paths")
                .and_then(serde_json::Value::as_array_mut)
            {
                for value in paths {
                    if let Some(path) = value.as_str() {
                        if let Ok(relative) = Path::new(path).strip_prefix(legacy) {
                            *value = serde_json::to_value(destination.join(relative))?;
                        }
                    }
                }
            }
        }
        Some(serde_json::to_vec(&history)?)
    } else {
        None
    };
    if destination.exists() {
        let entries = fs::read_dir(destination)?.collect::<std::io::Result<Vec<_>>>()?;
        anyhow::ensure!(
            entries
                .iter()
                .all(|entry| entry.file_name() == "startup-error.txt"),
            "The new Cult Copy data folder is not empty; preserve both folders before migrating"
        );
        for entry in entries {
            fs::remove_file(entry.path())?;
        }
        fs::remove_dir(destination)?;
    }
    fs::create_dir_all(destination.parent().context("Invalid data folder")?)?;
    if let Some(history) = history {
        fs::write(legacy.join(temporary), history)?;
    }
    if let Err(error) = fs::rename(legacy, destination) {
        let _ = fs::remove_file(legacy.join(temporary));
        return Err(error).context("Cannot migrate the previous Cult Copy profile");
    }
    if destination.join(temporary).is_file() {
        if let Err(error) = fs::rename(
            destination.join(temporary),
            destination.join("history.json"),
        ) {
            fs::rename(destination, legacy)
                .context("Cannot restore the previous profile location")?;
            let _ = fs::remove_file(legacy.join(temporary));
            return Err(error).context("Cannot migrate clipboard history paths");
        }
    }
    Ok(())
}

pub fn set_autostart(enabled: bool) -> Result<()> {
    let exe = std::env::current_exe()?;
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        let mut command = std::process::Command::new("reg.exe");
        command.creation_flags(0x08000000);
        if enabled {
            command.args([
                "add",
                r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
                "/v",
                "DuetClipboard",
                "/t",
                "REG_SZ",
                "/d",
                &format!("\"{}\" --background", exe.display()),
                "/f",
            ]);
        } else {
            command.args([
                "delete",
                r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
                "/v",
                "DuetClipboard",
                "/f",
            ]);
        }
        let output = command.output()?;
        if enabled {
            anyhow::ensure!(output.status.success(), "Could not enable start at login");
        }
    }
    #[cfg(target_os = "linux")]
    {
        // AppImage's mounted executable disappears at logout. Launch the
        // persistent AppImage path instead when start-at-login is enabled.
        let exe = std::env::var_os("APPIMAGE")
            .map(PathBuf::from)
            .unwrap_or(exe);
        let base = directories::BaseDirs::new().context("Cannot locate home folder")?;
        let dir = base.config_dir().join("autostart");
        fs::create_dir_all(&dir)?;
        let path = dir.join("cult-copy.desktop");
        let legacy = dir.join("duet-clipboard.desktop");
        if legacy.is_file() {
            fs::remove_file(legacy)?;
        }
        if enabled {
            let command = exe
                .to_string_lossy()
                .replace('\\', "\\\\")
                .replace('"', "\\\"")
                .replace('`', "\\`")
                .replace('$', "\\$")
                .replace('%', "%%");
            fs::write(path, format!("[Desktop Entry]\nType=Application\nName=Cult Copy\nExec=\"{command}\" --background\nIcon=cult-copy\nStartupWMClass=cult-copy\nTerminal=false\nX-GNOME-Autostart-enabled=true\n"))?;
        } else if path.exists() {
            fs::remove_file(path)?;
        }
    }
    #[cfg(target_os = "macos")]
    {
        let base = directories::BaseDirs::new().context("Cannot locate home folder")?;
        let dir = base.home_dir().join("Library/LaunchAgents");
        fs::create_dir_all(&dir)?;
        let path = dir.join("io.duetclipboard.app.plist");
        if enabled {
            let escaped = exe
                .to_string_lossy()
                .replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;");
            fs::write(path, format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?><!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\"><plist version=\"1.0\"><dict><key>Label</key><string>io.duetclipboard.app</string><key>ProgramArguments</key><array><string>{escaped}</string><string>--background</string></array><key>RunAtLoad</key><true/></dict></plist>"))?;
        } else if path.exists() {
            fs::remove_file(path)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(target_os = "linux")]
    #[test]
    fn migration_removes_the_launcher_that_kept_opening_the_previous_user_install() {
        let root = tempfile::tempdir().unwrap();
        let data = root.path().join("data");
        let config = root.path().join("config");
        let old = data.join("applications/duet-clipboard.desktop");
        let startup = config.join("autostart/duet-clipboard.desktop");
        let current = data.join("applications/cult-copy.desktop");
        for path in [&old, &startup, &current] {
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, b"launcher").unwrap();
        }
        let unrelated = config.join("autostart/other-app.desktop");
        fs::write(&unrelated, b"other app").unwrap();
        remove_legacy_launchers(&data, &config).unwrap();
        assert!(!old.exists());
        assert!(!startup.exists());
        assert_eq!(fs::read(current).unwrap(), b"launcher");
        assert_eq!(fs::read(unrelated).unwrap(), b"other app");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn profile_migration_preserves_identity_cached_files_and_external_sources() {
        let root = tempfile::tempdir().unwrap();
        let legacy = root.path().join("previous");
        let destination = root.path().join("cultcopy");
        let mut original = Config::load(&legacy).unwrap();
        original
            .trusted
            .insert("paired-device".into(), "paired name".into());
        original.save(&legacy).unwrap();
        let cached = legacy.join("received/event/file.txt");
        fs::create_dir_all(cached.parent().unwrap()).unwrap();
        fs::write(&cached, b"received bytes").unwrap();
        let external = root.path().join("original-file.txt");
        fs::write(&external, b"source bytes").unwrap();
        fs::write(
            legacy.join("history.json"),
            serde_json::to_vec(&serde_json::json!([
                {"paths": [cached, external], "preview": "text is not rewritten"}
            ]))
            .unwrap(),
        )
        .unwrap();
        migrate_profile(&legacy, &destination).unwrap();
        assert!(!legacy.exists());
        let migrated = Config::load(&destination).unwrap();
        assert_eq!(migrated.id(), original.id());
        assert_eq!(migrated.private_key, original.private_key);
        assert_eq!(migrated.trusted, original.trusted);
        assert_eq!(
            fs::read(destination.join("received/event/file.txt")).unwrap(),
            b"received bytes"
        );
        let history: serde_json::Value =
            serde_json::from_slice(&fs::read(destination.join("history.json")).unwrap()).unwrap();
        assert_eq!(
            history[0]["paths"][0],
            destination
                .join("received/event/file.txt")
                .to_str()
                .unwrap()
        );
        assert_eq!(history[0]["paths"][1], external.to_str().unwrap());
        assert_eq!(history[0]["preview"], "text is not rewritten");
        // A second migration never overwrites the new installation's identity.
        let other = Config::load(&legacy).unwrap();
        migrate_profile(&legacy, &destination).unwrap();
        assert_eq!(Config::load(&destination).unwrap().id(), original.id());
        assert_eq!(Config::load(&legacy).unwrap().id(), other.id());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn interrupted_profile_migration_finishes_history_rebasing_on_next_start() {
        let root = tempfile::tempdir().unwrap();
        let legacy = root.path().join("previous");
        let destination = root.path().join("cultcopy");
        let original = Config::load(&destination).unwrap();
        fs::write(destination.join("history.json"), b"old paths").unwrap();
        fs::write(destination.join("history.migration.tmp"), b"rebased paths").unwrap();
        migrate_profile(&legacy, &destination).unwrap();
        assert_eq!(
            fs::read(destination.join("history.json")).unwrap(),
            b"rebased paths"
        );
        assert!(!destination.join("history.migration.tmp").exists());
        assert_eq!(Config::load(&destination).unwrap().id(), original.id());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn profile_migration_refuses_a_running_previous_instance() {
        let root = tempfile::tempdir().unwrap();
        let legacy = root.path().join("previous");
        let destination = root.path().join("cultcopy");
        let original = Config::load(&legacy).unwrap();
        let lock = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(legacy.join("instance.lock"))
            .unwrap();
        fs2::FileExt::try_lock_exclusive(&lock).unwrap();
        assert!(migrate_profile(&legacy, &destination).is_err());
        assert!(!destination.exists());
        assert_eq!(Config::load(&legacy).unwrap().id(), original.id());
    }

    #[test]
    fn settings_and_identity_persist_and_limits_are_clamped() {
        let dir = tempfile::tempdir().unwrap();
        let mut config = Config::load(dir.path()).unwrap();
        let identity = config.public_key.clone();
        config.auto_receive_limit = 1;
        config.ask_before_receiving = true;
        config.save(dir.path()).unwrap();
        let mut loaded = Config::load(dir.path()).unwrap();
        assert_eq!(loaded.public_key, identity);
        assert_eq!(loaded.auto_receive_limit, MIN_LIMIT);
        assert!(loaded.ask_before_receiving);
        loaded.auto_receive_limit = u64::MAX;
        loaded.save(dir.path()).unwrap();
        assert_eq!(
            Config::load(dir.path()).unwrap().auto_receive_limit,
            MAX_LIMIT
        );
    }
}
