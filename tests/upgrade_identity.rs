//! Compatibility contracts from the last release under the original name.
//! Changing the brand must not create a new installed app, profile, or peer.
use cult_copy::config::{self, Config};

#[test]
fn existing_profiles_keep_their_location_device_keys_and_pairings() {
    let legacy = directories::ProjectDirs::from("io", "DuetClipboard", "DuetClipboard").unwrap();
    assert_eq!(config::default_dir().unwrap(), legacy.data_local_dir());
    let dir = tempfile::tempdir().unwrap();
    let private_key = "01".repeat(32);
    let public_key = "02".repeat(32);
    let settings = serde_json::json!({
        "name": "Previously paired computer",
        "private_key": private_key,
        "public_key": public_key,
        "trusted": { "existing-peer": "03".repeat(32) },
        "auto_receive_limit": 12_000_000,
        "ask_before_receiving": true,
        "start_at_login": true,
        "clock": 42
    });
    std::fs::write(dir.path().join("settings.json"), settings.to_string()).unwrap();
    let loaded = Config::load(dir.path()).unwrap();
    assert_eq!(loaded.private_key, private_key);
    assert_eq!(loaded.public_key, public_key);
    assert_eq!(loaded.trusted["existing-peer"], "03".repeat(32));
    assert_eq!(loaded.auto_receive_limit, 12_000_000);
    assert!(loaded.ask_before_receiving && loaded.start_at_login);
    assert_eq!(loaded.clock, 42);
    assert_eq!(Config::load(dir.path()).unwrap().id(), loaded.id());
}

#[test]
fn installers_and_discovery_retain_the_previous_release_identity() {
    let windows = include_str!("../packaging/windows.iss");
    assert!(windows.contains("AppId={{736A9FD9-50C7-43B8-9E1C-A9AA51AF943E}"));
    assert!(windows.contains("DefaultDirName={localappdata}\\Programs\\DuetClipboard"));
    assert!(windows.contains("AppUserModelID: \"io.duetclipboard.app\""));
    assert!(windows.contains("AppName=Cult Copy"));
    let mac = include_str!("../packaging/Info.plist");
    assert!(mac.contains("<key>CFBundleIdentifier</key><string>io.duetclipboard.app</string>"));
    assert!(mac.contains("<key>CFBundleDisplayName</key><string>Cult Copy</string>"));
    assert!(include_str!("../packaging/cult-copy.spec").contains("Name:           duet-clipboard"));
    let launcher = include_str!("../packaging/cult-copy.desktop");
    assert!(launcher.contains("Name=Cult Copy\n"));
    assert!(launcher.contains("StartupWMClass=duet-clipboard\n"));
    assert_eq!(cult_copy::network::SERVICE, "_duetclip._tcp.local.");
    assert!(include_str!("../src/protocol.rs").contains(".prologue(b\"duet-clipboard/v1\")"));
}

#[test]
#[cfg(feature = "desktop")]
fn legacy_executable_reports_the_new_brand_and_version() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_duet-clipboard"))
        .arg("--version")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap().trim(),
        format!("Cult Copy {}", env!("CARGO_PKG_VERSION"))
    );
}
