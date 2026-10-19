use crate::model::{ClipboardNotice, Kind, NoticePhase};
use std::sync::mpsc;

pub const APP_ID: &str = "io.duetclipboard.app";

pub fn message(notice: &ClipboardNotice) -> String {
    let icon = match notice.kind {
        Kind::Text => "📝",
        Kind::Image => "🖼",
        Kind::Files => "📎",
    };
    let status = match notice.phase {
        NoticePhase::ApprovalNeeded => "Approval needed",
        NoticePhase::Receiving => "Receiving",
        NoticePhase::Ready => "Ready to paste",
        NoticePhase::Saved => "Saved · newer clipboard kept",
        NoticePhase::Declined => "Declined",
        NoticePhase::Cancelled => "Transfer cancelled",
        NoticePhase::Failed => "Transfer failed",
    };
    format!("{icon} {status}")
}

pub fn start() -> mpsc::Sender<ClipboardNotice> {
    let (sender, receiver) = mpsc::channel::<ClipboardNotice>();
    std::thread::Builder::new()
        .name("clipboard-notifications".into())
        .spawn(move || {
            #[cfg(target_os = "macos")]
            let _ = notify_rust::set_application(APP_ID);
            while let Ok(notice) = receiver.recv() {
                // The persistent tray indicator shows an in-flight automatic copy. Its
                // banner appears when ready, so ordinary text copies get one banner.
                if matches!(
                    notice.phase,
                    NoticePhase::Receiving | NoticePhase::Declined | NoticePhase::Cancelled
                ) {
                    continue;
                }
                let mut notification = notify_rust::Notification::new();
                notification
                    .appname("Cult Copy")
                    .summary("New clipboard")
                    .body(&message(&notice))
                    .timeout(3500);
                #[cfg(target_os = "windows")]
                notification.app_id(APP_ID);
                #[cfg(target_os = "linux")]
                notification
                    .hint(notify_rust::Hint::SuppressSound(true))
                    .icon(match notice.kind {
                        Kind::Text => "text-x-generic",
                        Kind::Image => "image-x-generic",
                        Kind::Files => "application-octet-stream",
                    });
                if let Err(error) = notification.show() {
                    tracing::warn!("Desktop notification unavailable: {error}");
                }
            }
        })
        .expect("Could not start notification worker");
    sender
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn notifications_contain_only_a_type_icon_and_status() {
        for kind in [Kind::Text, Kind::Image, Kind::Files] {
            let notice = ClipboardNotice {
                event: uuid::Uuid::new_v4(),
                kind,
                phase: NoticePhase::ApprovalNeeded,
            };
            let text = message(&notice);
            assert!(text.ends_with("Approval needed"));
            assert!(!text.contains("filename") && !text.contains(&notice.event.to_string()));
        }
    }
}
