//! These tests change the actual desktop clipboard. Run only on a disposable
//! desktop/session, such as the native CI runners: cargo test --test native_clipboard -- --ignored
use cult_copy::{
    clipboard::{Captured, SystemClipboard},
    model::{Kind, TextPayload},
};

fn assert_file_capture(
    clipboard: &SystemClipboard,
    paths: &[std::path::PathBuf],
    root: &std::path::Path,
) {
    let captured = clipboard
        .snapshot()
        .unwrap()
        .content
        .expect("Missing copied files");
    match &captured {
        Captured::Files(actual) => assert_eq!(actual, paths),
        other => panic!("Expected files, captured {other:?}"),
    }
    let outgoing = cult_copy::storage::prepare(
        captured,
        cult_copy::model::Version {
            counter: 1,
            origin: "a".repeat(64),
        },
        root,
    )
    .unwrap();
    assert_eq!(
        outgoing.offer.kind,
        Kind::Files,
        "The clipboard tab must show FILE"
    );
    assert!(
        outgoing.preview.is_none(),
        "Files must not have URL text previews"
    );
}

#[test]
#[ignore = "changes the real clipboard; run on a disposable desktop session"]
fn native_text_image_file_and_folder_clipboards_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let (bus, _receiver) = tokio::sync::mpsc::channel(32);
    let clipboard = SystemClipboard::new(bus).unwrap();
    #[cfg(target_os = "macos")]
    {
        use clipboard_rs::{Clipboard, ClipboardContent, ClipboardContext};
        let context = ClipboardContext::new().unwrap();
        context
            .set(vec![
                ClipboardContent::Text("Apple remote clipboard placeholder".into()),
                ClipboardContent::Other("com.apple.is-remote-clipboard".into(), vec![]),
                ClipboardContent::Other(
                    cult_copy::clipboard::MARKER.into(),
                    uuid::Uuid::new_v4().as_bytes().to_vec(),
                ),
            ])
            .unwrap();
        let snapshot = clipboard.snapshot().unwrap();
        assert!(snapshot.ignored);
        assert!(snapshot.content.is_none());
        assert!(snapshot.marker.is_none());
    }
    let text_path = dir.path().join("text.json");
    std::fs::write(
        &text_path,
        serde_json::to_vec(&TextPayload {
            text: "Cross-platform clipboard — café 😀".into(),
            html: Some("<b>Clipboard</b>".into()),
            rtf: Some("{\\rtf1 Clipboard}".into()),
        })
        .unwrap(),
    )
    .unwrap();
    let id = uuid::Uuid::new_v4();
    clipboard.apply(&Kind::Text, &[text_path], id).unwrap();
    let snapshot = clipboard.snapshot().unwrap();
    assert!(!snapshot.ignored);
    assert_eq!(snapshot.marker, Some(id));
    match snapshot.content {
        Some(Captured::Text(text)) => {
            assert_eq!(text.text, "Cross-platform clipboard — café 😀");
            // Windows adds its standard HTML document/fragment envelope. Check
            // that the original formatting survives on every native backend.
            assert!(text
                .html
                .as_deref()
                .is_some_and(|html| html.contains("<b>Clipboard</b>")));
            assert!(text
                .rtf
                .as_deref()
                .is_some_and(|rtf| rtf.contains("Clipboard")));
        }
        _ => panic!("Missing native text clipboard"),
    }

    let image_path = dir.path().join("image.png");
    let pixels = image::RgbaImage::from_pixel(24, 16, image::Rgba([70, 140, 220, 255]));
    pixels.save(&image_path).unwrap();
    clipboard
        .apply(&Kind::Image, &[image_path], uuid::Uuid::new_v4())
        .unwrap();
    match clipboard.snapshot().unwrap().content {
        Some(Captured::Image(bytes)) => {
            assert_eq!(image::load_from_memory(&bytes).unwrap().to_rgba8(), pixels)
        }
        _ => panic!("Missing native image clipboard"),
    }

    let mut paths = vec![
        dir.path().join("file with spaces.txt"),
        dir.path().join("café.txt"),
    ];
    for path in &paths {
        std::fs::write(path, b"copied file").unwrap();
    }
    let folder = dir.path().join("folder café");
    std::fs::create_dir_all(folder.join("nested/empty")).unwrap();
    std::fs::write(folder.join("nested/file.txt"), b"nested file").unwrap();
    paths.push(folder);
    clipboard
        .apply(&Kind::Files, &paths, uuid::Uuid::new_v4())
        .unwrap();
    assert_file_capture(&clipboard, &paths, &dir.path().join("captured"));
    #[cfg(target_os = "windows")]
    {
        use clipboard_rs::{Clipboard, ClipboardContent, ClipboardContext};
        let context = ClipboardContext::new().unwrap();
        let url_text = paths
            .iter()
            .map(|path| url::Url::from_file_path(path).unwrap().to_string())
            .collect::<Vec<_>>()
            .join("\n");
        // Explorer-style CF_HDROP must take precedence even when URL text was
        // published first. Verify the transfer metadata used by the UI as well.
        context
            .set(vec![
                ClipboardContent::Text(url_text.clone()),
                ClipboardContent::Files(
                    paths
                        .iter()
                        .map(|path| path.to_str().unwrap().to_owned())
                        .collect(),
                ),
            ])
            .unwrap();
        assert_file_capture(&clipboard, &paths, &dir.path().join("captured"));

        // Another clipboard reader can hold OpenClipboard after the copy event.
        // The old immediate read failed here and never retried that file copy.
        let (locked, ready) = std::sync::mpsc::channel();
        let holder = std::thread::spawn(move || {
            let _clipboard = clipboard_win::Clipboard::new_attempts(10).unwrap();
            locked.send(()).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(60));
        });
        ready
            .recv_timeout(std::time::Duration::from_secs(2))
            .unwrap();
        assert_file_capture(&clipboard, &paths, &dir.path().join("captured"));
        holder.join().unwrap();

        // Some shell sources expose only the legacy Unicode file-name format.
        let filename = paths[0]
            .to_str()
            .unwrap()
            .encode_utf16()
            .chain([0])
            .flat_map(u16::to_le_bytes)
            .collect();
        context
            .set(vec![
                ClipboardContent::Text(url_text),
                ClipboardContent::Other("FileNameW".into(), filename),
            ])
            .unwrap();
        assert_file_capture(&clipboard, &paths[..1], &dir.path().join("captured"));
        // Restore a file-only clipboard for the independent format checks below.
        clipboard
            .apply(&Kind::Files, &paths, uuid::Uuid::new_v4())
            .unwrap();
    }
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    {
        use clipboard_rs::{Clipboard, ClipboardContent, ClipboardContext, ContentFormat};
        let context = ClipboardContext::new().unwrap();
        let uris = paths
            .iter()
            .map(|path| url::Url::from_file_path(path).unwrap().to_string())
            .collect::<Vec<_>>();
        // Check what independent paste targets receive, not just our own decoder.
        #[cfg(target_os = "linux")]
        assert_eq!(
            context.get_buffer("text/uri-list").unwrap(),
            format!("{}\r\n", uris.join("\r\n")).as_bytes()
        );
        assert!(
            !context.has(ContentFormat::Text),
            "File copies must not degrade into URL text"
        );
        for format in [
            "text/uri-list",
            "x-special/gnome-copied-files",
            "x-special/nautilus-clipboard",
        ] {
            // A file selection can carry a typed URI list plus plain URL text
            // without the backend's preferred native file type (e.g. CF_HDROP).
            let bytes = if format == "text/uri-list" {
                uris.join("\r\n")
            } else {
                format!("copy\n{}", uris.join("\n"))
            };
            context
                .set(vec![
                    ClipboardContent::Other(format.into(), bytes.into_bytes()),
                    ClipboardContent::Text(uris.join("\n")),
                ])
                .unwrap();
            assert_file_capture(&clipboard, &paths, &dir.path().join("captured"));
        }
        context
            .set_text("file:///tmp/a-deliberately-copied-url".into())
            .unwrap();
        assert!(
            matches!(
                clipboard.snapshot().unwrap().content,
                Some(Captured::Text(_))
            ),
            "Ordinary URL text must stay text"
        );
    }
}
