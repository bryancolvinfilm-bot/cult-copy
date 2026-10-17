use super::*;
use crate::{clipboard::Snapshot, model::TextPayload};
use std::sync::atomic::AtomicUsize;

#[derive(Default)]
struct Memory {
    content: Option<Captured>,
    marker: Option<Uuid>,
}

struct TestClipboard {
    memory: Arc<Mutex<Memory>>,
    writes: Arc<AtomicUsize>,
    bus: Bus,
}

impl ClipboardAccess for TestClipboard {
    fn snapshot(&self) -> Result<Snapshot> {
        let memory = self.memory.lock().unwrap();
        Ok(Snapshot {
            content: memory.content.clone(),
            marker: memory.marker,
        })
    }
    fn apply(&self, kind: &Kind, paths: &[PathBuf], id: Uuid) -> Result<()> {
        let content = match kind {
            Kind::Text => Captured::Text(storage::text_preview(&paths[0])?),
            Kind::Image => Captured::Image(std::fs::read(&paths[0])?),
            Kind::Files => Captured::Files(paths.to_vec()),
        };
        *self.memory.lock().unwrap() = Memory {
            content: Some(content),
            marker: Some(id),
        };
        self.writes.fetch_add(1, Ordering::Relaxed);
        let _ = self.bus.try_send(Command::ClipboardChanged);
        Ok(())
    }
}

struct Computer {
    dir: tempfile::TempDir,
    config: Config,
    handle: Handle,
    memory: Arc<Mutex<Memory>>,
    writes: Arc<AtomicUsize>,
}

impl Computer {
    fn new(ask: bool, limit: u64) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut config = Config::load(dir.path()).unwrap();
        config.ask_before_receiving = ask;
        config.auto_receive_limit = limit;
        let (mut engine, handle, receiver) = create_engine(config.clone(), dir.path().to_owned());
        let memory = Arc::new(Mutex::new(Memory::default()));
        let writes = Arc::new(AtomicUsize::new(0));
        engine.clipboard = Some(Box::new(TestClipboard {
            memory: memory.clone(),
            writes: writes.clone(),
            bus: handle.bus.clone(),
        }));
        tokio::spawn(engine.run(receiver));
        Self {
            dir,
            config,
            handle,
            memory,
            writes,
        }
    }
    fn copy(&self, captured: Captured) {
        *self.memory.lock().unwrap() = Memory {
            content: Some(captured),
            marker: None,
        };
        self.handle.send(Command::ClipboardChanged);
    }
    fn text(&self, value: &str) {
        self.copy(Captured::Text(TextPayload {
            text: value.into(),
            html: None,
            rtf: None,
        }));
    }
    fn current_text(&self) -> String {
        match &self.memory.lock().unwrap().content {
            Some(Captured::Text(text)) => text.text.clone(),
            _ => String::new(),
        }
    }
}

async fn wait_for(handle: &Handle, predicate: impl Fn(&View) -> bool) {
    tokio::time::timeout(Duration::from_secs(8), async {
        loop {
            if predicate(&handle.view.lock().unwrap()) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap_or_else(|_| {
        let view = handle.view.lock().unwrap();
        panic!(
            "Timed out: peers={:?}; history={:?}; notices={:?}",
            view.peers
                .iter()
                .map(|p| (&p.name, p.connected, p.ready))
                .collect::<Vec<_>>(),
            view.history
                .iter()
                .map(|i| (&i.offer.title, &i.status))
                .collect::<Vec<_>>(),
            view.notices
        );
    });
}

async fn connect(a: &Computer, b: &Computer) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let config = a.config.clone();
    let bus = a.handle.bus.clone();
    tokio::spawn(async move {
        let stream = tokio::net::TcpStream::connect(address).await.unwrap();
        let _ = network::connection(stream, config, true, None, bus).await;
    });
    let (stream, _) = listener.accept().await.unwrap();
    let config = b.config.clone();
    let bus = b.handle.bus.clone();
    tokio::spawn(async move {
        let _ = network::connection(stream, config, false, None, bus).await;
    });
    wait_for(&a.handle, |v| {
        v.peers
            .iter()
            .any(|p| p.connected && p.name == b.config.name)
    })
    .await;
    wait_for(&b.handle, |v| {
        v.peers
            .iter()
            .any(|p| p.connected && p.name == a.config.name)
    })
    .await;
    assert_eq!(
        a.handle.view.lock().unwrap().peers[0].code,
        b.handle.view.lock().unwrap().peers[0].code
    );
}

async fn trust(a: &Computer, b: &Computer) {
    a.handle.send(Command::Trust(b.config.id()));
    b.handle.send(Command::Trust(a.config.id()));
    wait_for(&a.handle, |v| v.peers.iter().any(|p| p.ready)).await;
    wait_for(&b.handle, |v| v.peers.iter().any(|p| p.ready)).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn encrypted_peers_require_pairing_and_sync_a_b_a_without_echo() {
    let a = Computer::new(false, crate::model::DEFAULT_LIMIT);
    let b = Computer::new(false, crate::model::DEFAULT_LIMIT);
    connect(&a, &b).await;
    a.text("Before pairing");
    wait_for(&a.handle, |v| !v.history.is_empty()).await;
    assert!(b.handle.view.lock().unwrap().history.is_empty());
    assert_eq!(b.writes.load(Ordering::Relaxed), 0);
    trust(&a, &b).await;
    for (index, value) in ["hello", "goodbye", "hello"].iter().enumerate() {
        a.text(value);
        wait_for(&b.handle, |v| {
            v.history
                .iter()
                .filter(|i| i.status == Status::Ready)
                .count()
                == index + 1
        })
        .await;
        assert_eq!(b.current_text(), *value);
    }
    assert_eq!(
        a.writes.load(Ordering::Relaxed),
        0,
        "Incoming writes must not echo back to their source"
    );
    assert_eq!(b.writes.load(Ordering::Relaxed), 3);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ask_for_everything_blocks_even_small_text_until_this_receiver_accepts() {
    let a = Computer::new(false, crate::model::DEFAULT_LIMIT);
    let b = Computer::new(true, crate::model::DEFAULT_LIMIT);
    connect(&a, &b).await;
    trust(&a, &b).await;
    a.text("Small text still requires approval");
    wait_for(&b.handle, |v| {
        v.history
            .first()
            .is_some_and(|i| i.status == Status::AwaitingApproval)
    })
    .await;
    assert_eq!(b.writes.load(Ordering::Relaxed), 0);
    assert_eq!(
        b.handle
            .view
            .lock()
            .unwrap()
            .latest_incoming
            .as_ref()
            .unwrap()
            .phase,
        crate::model::NoticePhase::ApprovalNeeded
    );
    assert!(
        !b.dir.path().join("received").exists(),
        "No payload should be downloaded before consent"
    );
    let id = b.handle.view.lock().unwrap().history[0].offer.id;
    b.handle.send(Command::Receive(id));
    wait_for(&b.handle, |v| v.history[0].status == Status::Ready).await;
    assert_eq!(b.current_text(), "Small text still requires approval");
    assert_eq!(
        b.handle
            .view
            .lock()
            .unwrap()
            .latest_incoming
            .as_ref()
            .unwrap()
            .phase,
        crate::model::NoticePhase::Ready
    );
    // Receiving with consent does not disable this computer's normal outgoing copies.
    b.text("Outgoing still works");
    wait_for(&a.handle, |v| {
        v.history.iter().any(|i| i.status == Status::Ready)
    })
    .await;
    assert_eq!(a.current_text(), "Outgoing still works");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn large_files_send_metadata_first_and_keep_a_newer_local_clipboard() {
    let a = Computer::new(false, crate::model::DEFAULT_LIMIT);
    let b = Computer::new(false, MIN_LIMIT);
    connect(&a, &b).await;
    trust(&a, &b).await;
    let original = a.dir.path().join("large file.bin");
    let bytes = vec![73u8; 3_000_000];
    std::fs::write(&original, &bytes).unwrap();
    a.copy(Captured::Files(vec![original.clone()]));
    wait_for(&b.handle, |v| {
        v.history
            .first()
            .is_some_and(|i| i.status == Status::AwaitingApproval)
    })
    .await;
    assert!(!b.dir.path().join("received").exists());
    let id = b.handle.view.lock().unwrap().history[0].offer.id;
    b.handle.send(Command::Receive(id));
    b.text("Keep this newer clipboard");
    wait_for(&b.handle, |v| {
        v.history
            .iter()
            .any(|i| i.offer.id == id && i.status == Status::Ready)
    })
    .await;
    assert_eq!(b.current_text(), "Keep this newer clipboard");
    let saved = b
        .handle
        .view
        .lock()
        .unwrap()
        .history
        .iter()
        .find(|i| i.offer.id == id)
        .unwrap()
        .paths[0]
        .clone();
    assert_eq!(std::fs::read(&saved).unwrap(), bytes);
    assert!(original.is_file(), "File copies must preserve the original");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn image_bytes_transfer_and_declining_never_changes_the_clipboard() {
    let a = Computer::new(false, crate::model::DEFAULT_LIMIT);
    let b = Computer::new(true, crate::model::DEFAULT_LIMIT);
    connect(&a, &b).await;
    trust(&a, &b).await;
    let image = image::RgbaImage::from_pixel(32, 24, image::Rgba([40, 100, 220, 255]));
    let mut png = std::io::Cursor::new(Vec::new());
    image.write_to(&mut png, image::ImageFormat::Png).unwrap();
    let png = png.into_inner();
    a.copy(Captured::Image(png.clone()));
    wait_for(&b.handle, |v| {
        v.history
            .first()
            .is_some_and(|i| i.status == Status::AwaitingApproval)
    })
    .await;
    let id = b.handle.view.lock().unwrap().history[0].offer.id;
    b.handle.send(Command::Decline(id));
    wait_for(&b.handle, |v| v.history[0].status == Status::Declined).await;
    assert_eq!(b.writes.load(Ordering::Relaxed), 0);
    a.text("Next item");
    wait_for(&b.handle, |v| v.history[0].offer.id != id).await;
    let next = b.handle.view.lock().unwrap().history[0].offer.id;
    b.handle.send(Command::Receive(next));
    wait_for(&b.handle, |v| v.history[0].status == Status::Ready).await;
    assert_eq!(b.current_text(), "Next item");
    a.copy(Captured::Image(png.clone()));
    wait_for(&b.handle, |v| {
        v.history[0].offer.kind == Kind::Image && v.history[0].status == Status::AwaitingApproval
    })
    .await;
    let image_id = b.handle.view.lock().unwrap().history[0].offer.id;
    b.handle.send(Command::Receive(image_id));
    wait_for(&b.handle, |v| v.history[0].status == Status::Ready).await;
    let received = b.memory.lock().unwrap();
    match &received.content {
        Some(Captured::Image(actual)) => assert_eq!(*actual, png),
        _ => panic!("Expected the received image"),
    }
}

#[tokio::test]
async fn enabling_approval_cancels_automatic_download_and_a_retry_ignores_old_chunks() {
    let dir = tempfile::tempdir().unwrap();
    let source_dir = tempfile::tempdir().unwrap();
    let config = Config::load(dir.path()).unwrap();
    let source = Config::load(source_dir.path()).unwrap();
    let (mut engine, handle, _rx) = create_engine(config, dir.path().to_owned());
    engine.clipboard = Some(Box::new(TestClipboard {
        memory: Arc::new(Mutex::new(Memory::default())),
        writes: Arc::new(AtomicUsize::new(0)),
        bus: handle.bus.clone(),
    }));
    let (control, mut requests) = mpsc::channel(32);
    let (bulk, _chunks) = mpsc::channel(4);
    engine.peers.insert(
        source.id(),
        Peer {
            key: source.public_key.clone(),
            name: "Source".into(),
            code: "verified".into(),
            session: Uuid::new_v4(),
            tx: PeerTx { control, bulk },
            local_trust: true,
            remote_trust: true,
            hello: true,
        },
    );
    let path = source_dir.path().join("file.txt");
    std::fs::write(&path, b"hello").unwrap();
    let item = storage::prepare(
        Captured::Files(vec![path]),
        Version {
            counter: 1,
            origin: source.id(),
        },
        &source_dir.path().join("outgoing"),
    )
    .unwrap();
    let event = item.offer.id;
    engine
        .trusted_message(source.id(), Message::Offer(item.offer))
        .unwrap();
    let first = match requests.recv().await.unwrap() {
        Message::Request { transfer, .. } => transfer,
        _ => panic!("Missing automatic request"),
    };
    engine.command(Command::SetAcceptAll(true)).unwrap();
    assert!(engine.receiving.is_empty());
    assert_eq!(engine.item(event).unwrap().status, Status::AwaitingApproval);
    assert!(matches!(requests.recv().await.unwrap(), Message::Cancel { id } if id == first));
    engine.command(Command::Receive(event)).unwrap();
    let second = match requests.recv().await.unwrap() {
        Message::Request { transfer, .. } => transfer,
        _ => panic!("Missing approved retry"),
    };
    assert_ne!(first, second);
    engine.command(Command::SetAcceptAll(true)).unwrap();
    assert_eq!(
        engine.receiving.len(),
        1,
        "An explicitly approved download remains approved"
    );
    engine
        .trusted_message(
            source.id(),
            Message::Chunk {
                id: first,
                part: 0,
                offset: 0,
                data: b"stale".to_vec(),
            },
        )
        .unwrap();
    engine
        .trusted_message(
            source.id(),
            Message::Failed {
                id: first,
                reason: "Late cancellation".into(),
            },
        )
        .unwrap();
    assert_eq!(engine.receiving.get(&second).unwrap().1.transferred(), 0);
    engine
        .trusted_message(
            source.id(),
            Message::Chunk {
                id: second,
                part: 0,
                offset: 0,
                data: b"hello".to_vec(),
            },
        )
        .unwrap();
    engine
        .trusted_message(source.id(), Message::Complete { id: second })
        .unwrap();
    assert_eq!(engine.item(event).unwrap().status, Status::Ready);
    assert!(engine.item(event).unwrap().applied);
}
