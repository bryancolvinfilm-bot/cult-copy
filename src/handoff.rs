//! Read-only, best-effort Handoff detection. Apple does not publish an API for
//! the Handoff switch, so absent, mixed, or unreadable preferences stay unknown.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HandoffStatus {
    On,
    Off,
    Unknown,
}

impl HandoffStatus {
    pub fn from_preferences(advertising: Option<bool>, receiving: Option<bool>) -> Self {
        match (advertising, receiving) {
            (Some(true), Some(true)) => Self::On,
            (Some(false), Some(false)) => Self::Off,
            _ => Self::Unknown,
        }
    }
}

#[cfg(target_os = "macos")]
pub fn status() -> HandoffStatus {
    use objc2_core_foundation::{
        CFPreferencesAppSynchronize, CFPreferencesGetAppBooleanValue, CFString,
    };

    let domain = CFString::from_str("com.apple.coreservices.useractivityd");
    // Refresh the effective preference search list, including this Mac's ByHost
    // values. We never set preferences, so there are no app changes to flush.
    if !CFPreferencesAppSynchronize(&domain) {
        return HandoffStatus::Unknown;
    }
    let read = |key: &str| {
        let mut valid = 0;
        // `valid` is writable for the duration of the Core Foundation call.
        let value = unsafe {
            CFPreferencesGetAppBooleanValue(&CFString::from_str(key), &domain, &mut valid)
        };
        (valid != 0).then_some(value)
    };
    HandoffStatus::from_preferences(
        read("ActivityAdvertisingAllowed"),
        read("ActivityReceivingAllowed"),
    )
}

pub fn settings_url(major_version: isize) -> &'static str {
    if major_version >= 13 {
        "x-apple.systempreferences:com.apple.AirDrop-Handoff-Settings.extension"
    } else {
        // Monterey keeps Handoff in the General preference pane.
        "x-apple.systempreferences:com.apple.preference.general"
    }
}

#[cfg(target_os = "macos")]
pub fn open_settings() -> anyhow::Result<()> {
    use anyhow::{ensure, Context};
    use objc2::rc::autoreleasepool;
    use objc2_app_kit::NSWorkspace;
    use objc2_foundation::{NSProcessInfo, NSURL};

    // This only runs from the app window after a user button click.
    autoreleasepool(|_| {
        let major = NSProcessInfo::processInfo()
            .operatingSystemVersion()
            .majorVersion;
        let url = NSURL::URLWithString(&objc2_foundation::NSString::from_str(settings_url(major)))
            .context("Cannot prepare the Handoff settings link")?;
        let workspace = NSWorkspace::sharedWorkspace();
        if !workspace.openURL(&url) {
            // URL routing varies by macOS version. Fall back to the settings app
            // and leave the manual route visible alongside the button.
            let output = std::process::Command::new("/usr/bin/open")
                .args(["-b", "com.apple.systempreferences"])
                .output()
                .context("Cannot open System Settings")?;
            ensure!(output.status.success(), "Cannot open System Settings");
        }
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_matching_explicit_preferences_confirm_handoff_state() {
        assert_eq!(
            HandoffStatus::from_preferences(Some(true), Some(true)),
            HandoffStatus::On
        );
        assert_eq!(
            HandoffStatus::from_preferences(Some(false), Some(false)),
            HandoffStatus::Off
        );
        for advertising in [None, Some(false), Some(true)] {
            for receiving in [None, Some(false), Some(true)] {
                if advertising.is_none() || receiving.is_none() || advertising != receiving {
                    assert_eq!(
                        HandoffStatus::from_preferences(advertising, receiving),
                        HandoffStatus::Unknown
                    );
                }
            }
        }
    }

    #[test]
    fn settings_link_supports_monterey_and_newer_macos() {
        assert!(settings_url(12).ends_with("com.apple.preference.general"));
        for version in [13, 14, 15, 26, 27] {
            assert!(settings_url(version).ends_with("com.apple.AirDrop-Handoff-Settings.extension"));
        }
    }
}
