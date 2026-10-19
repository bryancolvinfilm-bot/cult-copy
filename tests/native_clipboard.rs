//! These tests change the actual desktop clipboard. Run only on a disposable
//! desktop/session, such as the native CI runners: cargo test --test native_clipboard -- --ignored
use cult_copy::{
    clipboard::{Captured, SystemClipboard},
    model::{Kind, TextPayload},
};

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
    match clipboard.snapshot().unwrap().content {
        Some(Captured::Files(actual)) => assert_eq!(actual, paths),
        _ => panic!("Missing native file clipboard"),
    }
}
