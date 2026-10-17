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
    Ok(
        directories::ProjectDirs::from("io", "DuetClipboard", "DuetClipboard")
            .context("Cannot locate your application data folder")?
            .data_local_dir()
            .to_owned(),
    )
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
        let base = directories::BaseDirs::new().context("Cannot locate home folder")?;
        let dir = base.config_dir().join("autostart");
        fs::create_dir_all(&dir)?;
        let path = dir.join("duet-clipboard.desktop");
        if enabled {
            let command = exe
                .to_string_lossy()
                .replace('\\', "\\\\")
                .replace('"', "\\\"")
                .replace('`', "\\`")
                .replace('$', "\\$")
                .replace('%', "%%");
            fs::write(path, format!("[Desktop Entry]\nType=Application\nName=Duet Clipboard\nExec=\"{command}\" --background\nTerminal=false\nX-GNOME-Autostart-enabled=true\n"))?;
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
