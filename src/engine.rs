use crate::{
    clipboard::{Captured, ClipboardAccess, SystemClipboard},
    config::Config,
    model::{HistoryItem, Kind, Status, Version, HISTORY_LIMIT, MAX_LIMIT, MIN_LIMIT},
    network::{self, Event, PeerTx},
    protocol::Message,
    storage::{self, Outgoing, Receiver},
};
use anyhow::{ensure, Context, Result};
use std::{
    collections::{BTreeMap, HashMap, HashSet, VecDeque},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use tokio::{io::AsyncReadExt, sync::mpsc};
use uuid::Uuid;

pub type Bus = mpsc::Sender<Command>;
type Notify = Arc<dyn Fn(bool) + Send + Sync>;

pub enum Command {
    Open,
    Network(Event),
    ClipboardChanged,
    Prepared {
        revision: u64,
        result: Result<Outgoing>,
    },
    TransferSent {
        peer: String,
        id: Uuid,
        result: Result<()>,
    },
    Connect(String),
    ConnectAddress(String),
    Trust(String),
    Forget(String),
    Receive(Uuid),
    Decline(Uuid),
    Cancel(Uuid),
    Copy(Uuid),
    SetLimit(u64),
    SetAcceptAll(bool),
    SetPaused(bool),
    SetAutostart(bool),
    ClearHistory,
    DismissNotice(String),
    Tick,
}

#[derive(Clone, Default)]
pub struct PeerView {
    pub id: String,
    pub name: String,
    pub code: String,
    pub connected: bool,
    pub trusted: bool,
    pub ready: bool,
    pub address: Option<String>,
}

pub struct View {
    pub name: String,
    pub id: String,
    pub port: u16,
    pub limit: u64,
    pub ask_before_receiving: bool,
    pub paused: bool,
    pub autostart: bool,
    pub peers: Vec<PeerView>,
    pub history: Vec<HistoryItem>,
    pub notices: VecDeque<String>,
    pub clipboard: String,
    pub data_dir: PathBuf,
    pub latest_incoming: Option<crate::model::ClipboardNotice>,
}

#[derive(Clone)]
pub struct Handle {
    pub bus: Bus,
    pub view: Arc<Mutex<View>>,
    notifier: Arc<Mutex<Option<Notify>>>,
    notification_sender: Arc<Mutex<Option<std::sync::mpsc::Sender<crate::model::ClipboardNotice>>>>,
}

impl Handle {
    pub fn send(&self, command: Command) {
        match self.bus.try_send(command) {
            Ok(()) => (),
            Err(mpsc::error::TrySendError::Full(command)) => {
                let bus = self.bus.clone();
                let _ = std::thread::Builder::new()
                    .name("clipboard-command".into())
                    .spawn(move || {
                        let _ = bus.blocking_send(command);
                    });
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
                tracing::warn!("Background worker has stopped")
            }
        }
    }
    pub fn set_notifier(&self, callback: Notify) {
        *self.notifier.lock().unwrap() = Some(callback);
    }
    pub fn set_notification_sender(
        &self,
        sender: std::sync::mpsc::Sender<crate::model::ClipboardNotice>,
    ) {
        *self.notification_sender.lock().unwrap() = Some(sender);
    }
    fn clipboard_notice(&self, event: Uuid, kind: Kind, phase: crate::model::NoticePhase) {
        let notice = crate::model::ClipboardNotice { event, kind, phase };
        self.view.lock().unwrap().latest_incoming = Some(notice.clone());
        if let Some(sender) = self.notification_sender.lock().unwrap().as_ref() {
            let _ = sender.send(notice);
        }
        self.notify(false);
    }
    fn notify(&self, attention: bool) {
        if let Some(notify) = self.notifier.lock().unwrap().as_ref() {
            notify(attention);
        }
    }
}

struct Peer {
    key: String,
    name: String,
    code: String,
    session: Uuid,
    tx: PeerTx,
    local_trust: bool,
    remote_trust: bool,
    hello: bool,
}
impl Peer {
    fn ready(&self) -> bool {
        self.local_trust && self.remote_trust && self.hello
    }
}

struct Engine {
    config: Config,
    dir: PathBuf,
    handle: Handle,
    clipboard: Option<Box<dyn ClipboardAccess>>,
    clipboard_error: Option<String>,
    last_signature: Option<String>,
    last_written: Option<Uuid>,
    revision: u64,
    last_version: Option<Version>,
    seen: HashSet<Uuid>,
    seen_order: VecDeque<Uuid>,
    peers: BTreeMap<String, Peer>,
    discovered: BTreeMap<String, (String, String)>,
    connecting: HashMap<String, Instant>,
    connection_notices: HashMap<String, (String, Instant)>,
    outgoing: HashMap<Uuid, Arc<Outgoing>>,
    receiving: HashMap<Uuid, (String, Receiver)>,
    sending: HashMap<(String, Uuid), Arc<AtomicBool>>,
    history: VecDeque<HistoryItem>,
    last_repaint: Instant,
}

fn create_engine(config: Config, dir: PathBuf) -> (Engine, Handle, mpsc::Receiver<Command>) {
    let (bus, rx) = mpsc::channel(256);
    let handle = Handle {
        bus: bus.clone(),
        view: Arc::new(Mutex::new(View {
            name: config.name.clone(),
            id: config.id(),
            port: 0,
            limit: config.auto_receive_limit,
            ask_before_receiving: config.ask_before_receiving,
            paused: config.paused,
            autostart: config.start_at_login,
            peers: Vec::new(),
            history: Vec::new(),
            notices: VecDeque::new(),
            clipboard: "Starting clipboard monitor…".into(),
            data_dir: dir.clone(),
            latest_incoming: None,
        })),
        notifier: Arc::new(Mutex::new(None)),
        notification_sender: Arc::new(Mutex::new(None)),
    };
    let engine = Engine {
        config: config.clone(),
        dir,
        handle: handle.clone(),
        clipboard: None,
        clipboard_error: None,
        last_signature: None,
        last_written: None,
        revision: 0,
        last_version: None,
        seen: HashSet::new(),
        seen_order: VecDeque::new(),
        peers: BTreeMap::new(),
        discovered: BTreeMap::new(),
        connecting: HashMap::new(),
        connection_notices: HashMap::new(),
        outgoing: HashMap::new(),
        receiving: HashMap::new(),
        sending: HashMap::new(),
        history: VecDeque::new(),
        last_repaint: Instant::now(),
    };
    (engine, handle, rx)
}

pub fn start(config: Config, dir: PathBuf, runtime: &tokio::runtime::Runtime) -> Handle {
    let (engine, handle, rx) = create_engine(config.clone(), dir);
    let bus = handle.bus.clone();
    runtime.spawn(engine.run(rx));
    let network_bus = bus.clone();
    #[cfg(feature = "ui-test")]
    let diagnostics = std::env::var_os("DUET_LIFECYCLE_TEST_TO").is_some()
        || std::env::var_os("DUET_SCREENSHOT_TO").is_some();
    #[cfg(not(feature = "ui-test"))]
    let diagnostics = false;
    runtime.spawn(async move {
        if diagnostics {
            return;
        }
        if let Err(e) = network::start(config, network_bus.clone()).await {
            let _ = network_bus
                .send(Command::Network(Event::Warning(e.to_string())))
                .await;
        }
    });
    runtime.spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(10));
        loop {
            interval.tick().await;
            if bus.send(Command::Tick).await.is_err() {
                break;
            }
        }
    });
    handle
}

impl Engine {
    async fn run(mut self, mut rx: mpsc::Receiver<Command>) {
        self.restore_history();
        if self.clipboard.is_none() {
            match SystemClipboard::new(self.handle.bus.clone()) {
                Ok(clipboard) => {
                    self.last_signature = clipboard
                        .snapshot()
                        .ok()
                        .and_then(|s| s.content.map(|c| c.signature()));
                    self.handle.view.lock().unwrap().clipboard = clipboard.description().into();
                    self.clipboard = Some(Box::new(clipboard));
                }
                Err(e) => {
                    self.clipboard_error = Some(e.to_string());
                    self.notice(e.to_string());
                    self.handle.view.lock().unwrap().clipboard =
                        "Clipboard unavailable — check desktop permissions".into();
                }
            }
        }
        self.publish(false);
        while let Some(command) = rx.recv().await {
            let is_chunk = matches!(
                &command,
                Command::Network(Event::Message {
                    message: Message::Chunk { .. },
                    ..
                })
            );
            if let Err(e) = self.command(command) {
                self.notice(format!("{e:#}"));
            }
            if !is_chunk {
                self.save_history();
            }
            if !is_chunk || self.last_repaint.elapsed() > Duration::from_millis(100) {
                self.publish(false);
            }
        }
    }

    fn command(&mut self, command: Command) -> Result<()> {
        match command {
            Command::Open => self.publish(true),
            Command::Network(event) => self.network(event)?,
            Command::ClipboardChanged => self.capture()?,
            Command::Prepared { revision, result } => {
                if revision != self.revision || self.config.paused {
                    return Ok(());
                }
                let item = Arc::new(result?);
                self.remember(item.offer.id);
                if self
                    .last_version
                    .as_ref()
                    .is_none_or(|version| *version < item.offer.version)
                {
                    self.last_version = Some(item.offer.version.clone());
                }
                for previous in &mut self.history {
                    previous.applied = false;
                }
                self.history.push_front(HistoryItem {
                    offer: item.offer.clone(),
                    device: self.config.name.clone(),
                    peer: self.config.id(),
                    status: Status::Local,
                    paths: item.paths.clone(),
                    preview: item.preview.clone(),
                    transferred: item.offer.total,
                    started: None,
                    applied: true,
                });
                for peer in self.peers.values().filter(|p| p.ready()) {
                    peer.tx.send(Message::Offer(item.offer.clone()))?;
                }
                self.outgoing.insert(item.offer.id, item);
                self.trim_history();
            }
            Command::Connect(id) => {
                if self.peers.contains_key(&id) {
                    return Ok(());
                }
                let address = self
                    .discovered
                    .get(&id)
                    .context("Device is no longer nearby")?
                    .1
                    .clone();
                self.connecting.insert(id.clone(), Instant::now());
                self.clear_connection_notice(&id);
                network::connect(
                    address,
                    Some(id),
                    self.config.clone(),
                    self.handle.bus.clone(),
                );
            }
            Command::ConnectAddress(address) => {
                ensure!(
                    !address.is_empty() && address.len() <= 300,
                    "Enter a computer address and port"
                );
                self.clear_connection_notice(&address);
                network::connect(address, None, self.config.clone(), self.handle.bus.clone());
            }
            Command::Trust(id) => {
                let peer = self.peers.get_mut(&id).context("Device disconnected")?;
                ensure!(peer.hello, "Waiting for device information");
                self.config
                    .trusted
                    .insert(peer.key.clone(), peer.name.clone());
                peer.local_trust = true;
                peer.tx.send(Message::Trust)?;
                self.config.save(&self.dir)?;
            }
            Command::Forget(id) => {
                if let Some(peer) = self.peers.remove(&id) {
                    self.config.trusted.remove(&peer.key);
                } else {
                    self.config.trusted.retain(|key, _| {
                        blake3::hash(&hex::decode(key).unwrap_or_default())
                            .to_hex()
                            .as_str()
                            != id
                    });
                }
                self.disconnect_transfers(&id);
                self.config.save(&self.dir)?;
            }
            Command::Receive(id) => self.receive(id, true)?,
            Command::Decline(id) => {
                if let Some(item) = self.item_mut(id) {
                    if item.status == Status::AwaitingApproval {
                        item.status = Status::Declined;
                    }
                }
                if let Some(item) = self.item(id) {
                    self.handle.clipboard_notice(
                        id,
                        item.offer.kind.clone(),
                        crate::model::NoticePhase::Declined,
                    );
                }
            }
            Command::Cancel(id) => {
                let transfer = self
                    .receiving
                    .iter()
                    .find(|(_, (_, job))| job.offer.id == id)
                    .map(|(&transfer, _)| transfer);
                if let Some((peer_id, _job)) =
                    transfer.and_then(|transfer| self.receiving.remove(&transfer))
                {
                    if let Some(peer) = self.peers.get(&peer_id) {
                        let _ = peer.tx.send(Message::Cancel {
                            id: transfer.unwrap(),
                        });
                    }
                    if let Some(item) = self.item_mut(id) {
                        item.status = Status::Cancelled;
                    }
                    if let Some(item) = self.item(id) {
                        self.handle.clipboard_notice(
                            id,
                            item.offer.kind.clone(),
                            crate::model::NoticePhase::Cancelled,
                        );
                    }
                }
            }
            Command::Copy(id) => self.copy_history(id)?,
            Command::SetLimit(limit) => {
                self.config.auto_receive_limit = limit.clamp(MIN_LIMIT, MAX_LIMIT);
                self.config.save(&self.dir)?;
            }
            Command::SetAcceptAll(enabled) => {
                self.config.ask_before_receiving = enabled;
                if enabled {
                    let ids: Vec<_> = self
                        .receiving
                        .values()
                        .filter(|(_, job)| !job.explicit_approval)
                        .map(|(_, job)| job.offer.id)
                        .collect();
                    for id in ids {
                        self.command(Command::Cancel(id))?;
                        if let Some(item) = self.item_mut(id) {
                            item.status = Status::AwaitingApproval;
                        }
                        if let Some(item) = self.item(id) {
                            self.handle.clipboard_notice(
                                id,
                                item.offer.kind.clone(),
                                crate::model::NoticePhase::ApprovalNeeded,
                            );
                        }
                    }
                }
                self.config.save(&self.dir)?;
            }
            Command::SetPaused(paused) => {
                self.config.paused = paused;
                if paused {
                    for ((_, _), cancelled) in &self.sending {
                        cancelled.store(true, Ordering::Relaxed);
                    }
                    let ids: Vec<_> = self
                        .receiving
                        .values()
                        .map(|(_, job)| job.offer.id)
                        .collect();
                    for id in ids {
                        self.command(Command::Cancel(id))?;
                    }
                }
                self.config.save(&self.dir)?;
            }
            Command::SetAutostart(enabled) => {
                crate::config::set_autostart(enabled)?;
                self.config.start_at_login = enabled;
                self.config.save(&self.dir)?;
            }
            Command::ClearHistory => {
                // Never remove files backing the live clipboard or an active transfer.
                let removed: Vec<_> = self
                    .history
                    .iter()
                    .filter(|i| {
                        !i.applied
                            && !matches!(i.status, Status::Receiving | Status::AwaitingApproval)
                    })
                    .map(|i| i.offer.id)
                    .collect();
                self.history.retain(|i| {
                    i.applied || matches!(i.status, Status::Receiving | Status::AwaitingApproval)
                });
                for id in removed {
                    self.outgoing.remove(&id);
                    self.remove_cached(id);
                }
                self.notice(
                    "History cleared. Items backing the current clipboard remain available.".into(),
                );
            }
            Command::TransferSent { peer, id, result } => {
                self.sending.remove(&(peer.clone(), id));
                if let Err(error) = result {
                    if let Some(peer) = self.peers.get(&peer) {
                        let _ = peer.tx.send(Message::Failed {
                            id,
                            reason: error.to_string().chars().take(200).collect(),
                        });
                    }
                }
            }
            Command::DismissNotice(message) => {
                self.handle
                    .view
                    .lock()
                    .unwrap()
                    .notices
                    .retain(|notice| notice != &message);
                self.connection_notices
                    .retain(|_, (notice, _)| notice != &message);
            }
            Command::Tick => {
                let expired: Vec<_> = self
                    .connection_notices
                    .iter()
                    .filter(|(_, (_, time))| time.elapsed() >= Duration::from_secs(30))
                    .map(|(key, _)| key.clone())
                    .collect();
                for key in expired {
                    self.clear_connection_notice(&key);
                }
                for peer in self.peers.values() {
                    let _ = peer.tx.send(Message::Ping);
                }
                self.connecting
                    .retain(|_, started| started.elapsed() < Duration::from_secs(15));
                let own = self.config.id();
                for (id, (_name, address)) in &self.discovered {
                    let trusted = self.config.trusted.keys().any(|key| {
                        blake3::hash(&hex::decode(key).unwrap_or_default())
                            .to_hex()
                            .as_str()
                            == id
                    });
                    if trusted
                        && own < *id
                        && !self.peers.contains_key(id)
                        && !self.connecting.contains_key(id)
                    {
                        self.connecting.insert(id.clone(), Instant::now());
                        network::connect(
                            address.clone(),
                            Some(id.clone()),
                            self.config.clone(),
                            self.handle.bus.clone(),
                        );
                    }
                }
                if self.clipboard.is_none() {
                    if let Ok(clipboard) = SystemClipboard::new(self.handle.bus.clone()) {
                        self.handle.view.lock().unwrap().clipboard = clipboard.description().into();
                        self.clipboard = Some(Box::new(clipboard));
                        self.clipboard_error = None;
                    }
                }
            }
        }
        Ok(())
    }

    fn capture(&mut self) -> Result<()> {
        let snapshot = self
            .clipboard
            .as_ref()
            .context("Clipboard is unavailable")?
            .snapshot()?;
        let signature = snapshot.content.as_ref().map(Captured::signature);
        if signature == self.last_signature {
            return Ok(());
        }
        self.last_signature = signature;
        if snapshot.marker.is_some() && snapshot.marker == self.last_written {
            return Ok(());
        }
        self.revision = self.revision.saturating_add(1);
        if self.config.paused {
            return Ok(());
        }
        let Some(captured) = snapshot.content else {
            return Ok(());
        };
        self.config.clock = self.config.clock.saturating_add(1);
        self.config.save(&self.dir)?;
        let version = Version {
            counter: self.config.clock,
            origin: self.config.id(),
        };
        self.last_version = Some(version.clone());
        let revision = self.revision;
        let root = self.dir.join("outgoing");
        let bus = self.handle.bus.clone();
        tokio::spawn(async move {
            let result =
                tokio::task::spawn_blocking(move || storage::prepare(captured, version, &root))
                    .await
                    .unwrap_or_else(|e| Err(e.into()));
            let _ = bus.send(Command::Prepared { revision, result }).await;
        });
        Ok(())
    }

    fn network(&mut self, event: Event) -> Result<()> {
        match event {
            Event::Listening(port) => self.handle.view.lock().unwrap().port = port,
            Event::Warning(warning) => self.notice(warning),
            Event::DiscoveryLost(id) => {
                self.discovered.remove(&id);
                self.connecting.remove(&id);
                self.clear_connection_notice(&id);
            }
            Event::ConnectionFailed { id, address, error } => {
                // A parallel attempt can fail after another session succeeds.
                if id.as_ref().is_some_and(|id| self.peers.contains_key(id)) {
                    return Ok(());
                }
                let key = id.unwrap_or(address.clone());
                self.clear_connection_notice(&key);
                let message = format!("Could not connect to {address}: {error}");
                self.notice(message.clone());
                self.connection_notices
                    .insert(key, (message, Instant::now()));
            }
            Event::Discovered { id, name, address } => {
                self.discovered
                    .insert(id.clone(), (name, address.to_string()));
                if self.config.id() < id
                    && self.config.trusted.keys().any(|key| {
                        blake3::hash(&hex::decode(key).unwrap_or_default())
                            .to_hex()
                            .as_str()
                            == id
                    })
                    && !self.peers.contains_key(&id)
                    && !self.connecting.contains_key(&id)
                {
                    self.command(Command::Connect(id))?;
                }
            }
            Event::Connected {
                id,
                key,
                code,
                session,
                sender,
            } => {
                self.connecting.remove(&id);
                self.clear_connection_notice(&id);
                if let Some((_, address)) = self.discovered.get(&id) {
                    self.clear_connection_notice(&address.clone());
                }
                if self.peers.contains_key(&id) {
                    return Ok(());
                }
                let trusted = self.config.trusted.contains_key(&key);
                let name = self
                    .config
                    .trusted
                    .get(&key)
                    .cloned()
                    .unwrap_or_else(|| "Nearby computer".into());
                if trusted {
                    sender.send(Message::Trust)?;
                }
                self.peers.insert(
                    id,
                    Peer {
                        key,
                        name,
                        code,
                        session,
                        tx: sender,
                        local_trust: trusted,
                        remote_trust: false,
                        hello: false,
                    },
                );
                if !trusted {
                    self.publish(true);
                }
            }
            Event::Disconnected { id, session } => {
                if self
                    .peers
                    .get(&id)
                    .is_some_and(|peer| peer.session == session)
                {
                    self.peers.remove(&id);
                    self.disconnect_transfers(&id);
                }
            }
            Event::Message {
                id,
                session,
                message,
            } => {
                let Some(peer) = self.peers.get_mut(&id) else {
                    return Ok(());
                };
                if peer.session != session {
                    return Ok(());
                }
                match message {
                    Message::Hello { name, version } => {
                        ensure!(
                            version == crate::protocol::PROTOCOL_VERSION && name.len() <= 320,
                            "Incompatible peer version"
                        );
                        peer.name = name.chars().filter(|c| !c.is_control()).take(80).collect();
                        peer.hello = true;
                    }
                    Message::Trust => peer.remote_trust = true,
                    Message::Ping => (),
                    message => {
                        if !peer.ready() {
                            self.peers.remove(&id);
                            anyhow::bail!("Unpaired device attempted to exchange clipboard data; connection closed");
                        }
                        let request = if let Message::Request { transfer, .. } = &message {
                            Some(*transfer)
                        } else {
                            None
                        };
                        if let Err(error) = self.trusted_message(id.clone(), message) {
                            if let Some(transfer) = request {
                                if let Some(peer) = self.peers.get(&id) {
                                    let _ = peer.tx.send(Message::Failed {
                                        id: transfer,
                                        reason: error.to_string().chars().take(200).collect(),
                                    });
                                }
                            }
                            return Err(error);
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn trusted_message(&mut self, peer_id: String, message: Message) -> Result<()> {
        match message {
            Message::Offer(offer) => {
                if self.config.paused {
                    return Ok(());
                }
                offer.validate()?;
                ensure!(
                    offer.version.origin == peer_id,
                    "Offer origin does not match its authenticated sender"
                );
                if self.seen.contains(&offer.id)
                    || self
                        .last_version
                        .as_ref()
                        .is_some_and(|v| *v >= offer.version)
                {
                    return Ok(());
                }
                self.config.clock = self.config.clock.max(offer.version.counter);
                self.config.save(&self.dir)?;
                self.last_version = Some(offer.version.clone());
                self.remember(offer.id);
                let needs_approval = self.config.ask_before_receiving
                    || offer.needs_approval(self.config.auto_receive_limit);
                self.handle.clipboard_notice(
                    offer.id,
                    offer.kind.clone(),
                    if needs_approval {
                        crate::model::NoticePhase::ApprovalNeeded
                    } else {
                        crate::model::NoticePhase::Receiving
                    },
                );
                let id = offer.id;
                let name = self
                    .peers
                    .get(&peer_id)
                    .context("Peer disconnected")?
                    .name
                    .clone();
                self.history.push_front(HistoryItem {
                    offer,
                    device: name,
                    peer: peer_id,
                    status: Status::AwaitingApproval,
                    paths: Vec::new(),
                    preview: None,
                    transferred: 0,
                    started: None,
                    applied: false,
                });
                self.trim_history();
                if needs_approval {
                    self.publish(true);
                } else {
                    self.receive(id, false)?;
                }
            }
            Message::Request { id, transfer } => {
                ensure!(!self.config.paused, "Sharing is paused");
                ensure!(
                    self.sending.len() < 8,
                    "Too many uploads are running; try again shortly"
                );
                if self.sending.contains_key(&(peer_id.clone(), transfer)) {
                    return Ok(());
                }
                let item = self
                    .outgoing
                    .get(&id)
                    .context("This clipboard item is no longer available; copy it again")?
                    .clone();
                let peer = self.peers.get(&peer_id).context("Peer disconnected")?;
                let tx = peer.tx.clone();
                let cancel = Arc::new(AtomicBool::new(false));
                self.sending
                    .insert((peer_id.clone(), transfer), cancel.clone());
                let bus = self.handle.bus.clone();
                tokio::spawn(async move {
                    let result = send_transfer(item, transfer, tx, cancel).await;
                    let _ = bus
                        .send(Command::TransferSent {
                            peer: peer_id,
                            id: transfer,
                            result,
                        })
                        .await;
                });
            }
            Message::Chunk {
                id,
                part,
                offset,
                data,
            } => {
                let Some((owner, job)) = self.receiving.get_mut(&id) else {
                    return Ok(());
                };
                ensure!(*owner == peer_id, "Transfer sender mismatch");
                let result = job.write(part as usize, offset, &data);
                let count = job.transferred();
                let event = job.offer.id;
                if let Err(error) = result {
                    self.fail_receive(id, error.to_string());
                    return Ok(());
                }
                if let Some(item) = self.item_mut(event) {
                    item.transferred = count;
                }
            }
            Message::Complete { id } => {
                let Some((owner, job)) = self.receiving.remove(&id) else {
                    return Ok(());
                };
                ensure!(owner == peer_id, "Transfer sender mismatch");
                let revision = job.revision;
                let event = job.offer.id;
                match job.finish() {
                    Ok(paths) => {
                        let kind = self
                            .item(event)
                            .context("History item was removed")?
                            .offer
                            .kind
                            .clone();
                        let validation = (|| -> Result<Option<String>> {
                            if kind == Kind::Image {
                                crate::clipboard::check_image(&paths[0])?;
                            }
                            Ok(if kind == Kind::Text {
                                Some(
                                    storage::text_preview(&paths[0])?
                                        .text
                                        .chars()
                                        .take(10_000)
                                        .collect(),
                                )
                            } else {
                                None
                            })
                        })();
                        let preview = match validation {
                            Ok(preview) => preview,
                            Err(error) => {
                                if let Some(item) = self.item_mut(event) {
                                    item.status = Status::Failed(error.to_string());
                                }
                                self.remove_cached(event);
                                self.handle.clipboard_notice(
                                    event,
                                    kind,
                                    crate::model::NoticePhase::Failed,
                                );
                                return Err(error);
                            }
                        };
                        if let Some(item) = self.item_mut(event) {
                            item.paths = paths;
                            item.preview = preview;
                            item.transferred = item.offer.total;
                            item.status = Status::Ready;
                        }
                        // Process a local clipboard change that may have raced the last network chunk.
                        self.capture()?;
                        if self.revision == revision && !self.config.paused {
                            self.apply(event)?;
                        }
                        let applied = self.item(event).is_some_and(|item| item.applied);
                        self.handle.clipboard_notice(
                            event,
                            kind,
                            if applied {
                                crate::model::NoticePhase::Ready
                            } else {
                                crate::model::NoticePhase::Saved
                            },
                        );
                    }
                    Err(error) => {
                        if let Some(item) = self.item_mut(event) {
                            item.status = Status::Failed(error.to_string());
                        }
                    }
                }
            }
            Message::Cancel { id } => {
                if let Some(cancel) = self.sending.remove(&(peer_id.clone(), id)) {
                    cancel.store(true, Ordering::Relaxed);
                }
            }
            Message::Failed { id, reason }
                if self
                    .receiving
                    .get(&id)
                    .is_some_and(|(peer, _)| *peer == peer_id) =>
            {
                self.fail_receive(id, reason.chars().take(200).collect());
            }
            _ => (),
        }
        Ok(())
    }

    fn receive(&mut self, id: Uuid, explicit: bool) -> Result<()> {
        ensure!(
            !self.config.paused,
            "Resume sharing before receiving a file"
        );
        ensure!(
            self.receiving.len() < 4,
            "Four downloads are already running; wait for one to finish"
        );
        let item = self.item(id).context("Item is no longer available")?;
        ensure!(
            matches!(
                item.status,
                Status::AwaitingApproval | Status::Failed(_) | Status::Cancelled
            ),
            "This item is not awaiting a transfer"
        );
        let offer = item.offer.clone();
        let peer_id = item.peer.clone();
        let peer = self
            .peers
            .get(&peer_id)
            .context("Source computer is offline")?;
        ensure!(peer.ready(), "Source computer is not paired");
        self.revision = self.revision.saturating_add(1);
        let mut job = Receiver::new(offer, self.revision, &self.dir.join("received"))?;
        job.explicit_approval = explicit;
        let transfer = job.transfer;
        peer.tx.send(Message::Request { id, transfer })?;
        self.receiving.insert(transfer, (peer_id, job));
        if let Some(item) = self.item(id) {
            self.handle.clipboard_notice(
                id,
                item.offer.kind.clone(),
                crate::model::NoticePhase::Receiving,
            );
        }
        if let Some(item) = self.item_mut(id) {
            item.status = Status::Receiving;
            item.started = Some(Instant::now());
            item.transferred = 0;
        }
        Ok(())
    }

    fn apply(&mut self, id: Uuid) -> Result<()> {
        let item = self.item(id).context("Item is no longer available")?;
        self.clipboard
            .as_ref()
            .context("Clipboard is unavailable")?
            .apply(&item.offer.kind, &item.paths, id)?;
        self.last_written = Some(id);
        self.last_signature = self
            .clipboard
            .as_ref()
            .and_then(|c| c.snapshot().ok())
            .and_then(|s| s.content.map(|c| c.signature()));
        for item in &mut self.history {
            item.applied = item.offer.id == id;
        }
        Ok(())
    }

    fn copy_history(&mut self, id: Uuid) -> Result<()> {
        ensure!(
            !self.item(id).context("Item unavailable")?.paths.is_empty(),
            "Receive this item first"
        );
        self.revision = self.revision.saturating_add(1);
        self.apply(id)?;
        // A deliberate history selection is a fresh copy action, so A → B → A works.
        self.last_signature = None;
        self.last_written = None;
        self.capture()?;
        Ok(())
    }

    fn disconnect_transfers(&mut self, id: &str) {
        let incoming = self
            .receiving
            .iter()
            .filter(|(_, (peer, _))| peer == id)
            .map(|(&key, _)| key)
            .collect::<Vec<_>>();
        for key in incoming {
            self.fail_receive(
                key,
                "Source computer disconnected. Reconnect, then retry.".into(),
            );
        }
        for ((peer, _), flag) in &self.sending {
            if peer == id {
                flag.store(true, Ordering::Relaxed);
            }
        }
    }

    fn fail_receive(&mut self, id: Uuid, reason: String) {
        if let Some((peer, job)) = self.receiving.remove(&id) {
            if let Some(peer) = self.peers.get(&peer) {
                let _ = peer.tx.send(Message::Cancel { id });
            }
            if let Some(item) = self.item_mut(job.offer.id) {
                item.status = Status::Failed(reason);
            }
            self.handle.clipboard_notice(
                job.offer.id,
                job.offer.kind.clone(),
                crate::model::NoticePhase::Failed,
            );
        }
    }

    fn item(&self, id: Uuid) -> Option<&HistoryItem> {
        self.history.iter().find(|i| i.offer.id == id)
    }
    fn item_mut(&mut self, id: Uuid) -> Option<&mut HistoryItem> {
        self.history.iter_mut().find(|i| i.offer.id == id)
    }
    fn remember(&mut self, id: Uuid) {
        self.seen.insert(id);
        self.seen_order.push_back(id);
        while self.seen_order.len() > 2048 {
            if let Some(old) = self.seen_order.pop_front() {
                self.seen.remove(&old);
            }
        }
    }
    fn trim_history(&mut self) {
        while self.history.len() > HISTORY_LIMIT {
            let index = self
                .history
                .iter()
                .rposition(|i| !i.applied && i.status != Status::Receiving);
            if let Some(index) = index {
                let item = self.history.remove(index).unwrap();
                self.outgoing.remove(&item.offer.id);
                self.remove_cached(item.offer.id);
            } else {
                break;
            }
        }
    }
    fn clear_connection_notice(&mut self, key: &str) {
        if let Some((message, _)) = self.connection_notices.remove(key) {
            self.handle
                .view
                .lock()
                .unwrap()
                .notices
                .retain(|notice| notice != &message);
        }
    }

    fn notice(&self, message: String) {
        tracing::warn!("{message}");
        let mut view = self.handle.view.lock().unwrap();
        if view.notices.back() != Some(&message) {
            view.notices.push_back(message);
        }
        while view.notices.len() > 5 {
            view.notices.pop_front();
        }
    }
    fn publish(&mut self, attention: bool) {
        let mut peers = BTreeMap::<String, PeerView>::new();
        for (key, name) in &self.config.trusted {
            let id = blake3::hash(&hex::decode(key).unwrap_or_default())
                .to_hex()
                .to_string();
            peers.insert(
                id.clone(),
                PeerView {
                    id,
                    name: name.clone(),
                    trusted: true,
                    ..Default::default()
                },
            );
        }
        for (id, (name, address)) in &self.discovered {
            let peer = peers.entry(id.clone()).or_insert_with(|| PeerView {
                id: id.clone(),
                name: name.clone(),
                ..Default::default()
            });
            peer.address = Some(address.clone());
        }
        for (id, peer) in &self.peers {
            let row = peers.entry(id.clone()).or_insert_with(|| PeerView {
                id: id.clone(),
                ..Default::default()
            });
            row.name = peer.name.clone();
            row.code = peer.code.clone();
            row.connected = true;
            row.trusted = peer.local_trust;
            row.ready = peer.ready();
        }
        {
            let mut view = self.handle.view.lock().unwrap();
            view.peers = peers.into_values().collect();
            view.history = self.history.iter().cloned().collect();
            view.limit = self.config.auto_receive_limit;
            view.ask_before_receiving = self.config.ask_before_receiving;
            view.paused = self.config.paused;
            view.autostart = self.config.start_at_login;
        }
        self.last_repaint = Instant::now();
        self.handle.notify(attention);
    }

    fn save_history(&self) {
        if let Ok(bytes) = serde_json::to_vec(&self.history) {
            let temporary = self.dir.join("history.tmp");
            if std::fs::write(&temporary, bytes).is_ok() {
                let _ = std::fs::rename(temporary, self.dir.join("history.json"));
            }
        }
    }

    fn restore_history(&mut self) {
        if let Ok(bytes) = std::fs::read(self.dir.join("history.json")) {
            if bytes.len() > 2_000_000 {
                return;
            }
            if let Ok(items) = serde_json::from_slice::<VecDeque<HistoryItem>>(&bytes) {
                for mut item in items.into_iter().take(HISTORY_LIMIT) {
                    if item.offer.validate().is_err() {
                        continue;
                    }
                    if item.status == Status::Receiving {
                        item.status = Status::Failed(
                            "Download interrupted. Reconnect to the source and retry.".into(),
                        );
                    }
                    if !item.paths.is_empty() && item.paths.iter().any(|path| !path.is_file()) {
                        continue;
                    }
                    if item.peer == self.config.id() && !item.paths.is_empty() {
                        self.outgoing.insert(
                            item.offer.id,
                            Arc::new(Outgoing {
                                offer: item.offer.clone(),
                                paths: item.paths.clone(),
                                preview: item.preview.clone(),
                            }),
                        );
                    }
                    self.remember(item.offer.id);
                    self.history.push_back(item);
                }
            }
        }
    }

    fn remove_cached(&self, id: Uuid) {
        if !self.sending.is_empty() {
            return;
        }
        // The deletion targets are generated UUID directories beneath our own cache roots.
        for root in ["outgoing", "received"] {
            let _ = std::fs::remove_dir_all(self.dir.join(root).join(id.to_string()));
        }
    }
}

async fn send_transfer(
    item: Arc<Outgoing>,
    transfer: Uuid,
    tx: PeerTx,
    cancelled: Arc<AtomicBool>,
) -> Result<()> {
    for (index, path) in item.paths.iter().enumerate() {
        let mut file = tokio::fs::File::open(path)
            .await
            .context("Source file is no longer available; copy it again")?;
        let mut offset = 0u64;
        let mut hash = blake3::Hasher::new();
        loop {
            ensure!(!cancelled.load(Ordering::Relaxed), "Transfer cancelled");
            let mut data = vec![0u8; crate::protocol::CHUNK_SIZE];
            let n = file.read(&mut data).await?;
            if n == 0 {
                break;
            }
            data.truncate(n);
            hash.update(&data);
            tx.bulk
                .send(Message::Chunk {
                    id: transfer,
                    part: index as u32,
                    offset,
                    data,
                })
                .await?;
            offset += n as u64;
        }
        ensure!(
            offset == item.offer.parts[index].size
                && hash.finalize().to_hex().as_str() == item.offer.parts[index].hash,
            "Source file changed; copy it again"
        );
    }
    // Completion uses the bulk queue so it cannot overtake the last file chunk.
    tx.bulk.send(Message::Complete { id: transfer }).await?;
    Ok(())
}

#[cfg(test)]
mod tests;
