use crate::{
    config::Config,
    engine::{Bus, Command},
    protocol::{self, Message},
};
use anyhow::{Context, Result};
use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use std::{collections::HashMap, net::SocketAddr, time::Duration};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::mpsc,
};
use uuid::Uuid;

pub const SERVICE: &str = "_duetclip._tcp.local.";

#[derive(Clone)]
pub struct PeerTx {
    pub control: mpsc::Sender<Message>,
    pub bulk: mpsc::Sender<Message>,
}

impl PeerTx {
    pub fn send(&self, message: Message) -> Result<()> {
        self.control
            .try_send(message)
            .map_err(|_| anyhow::anyhow!("Peer is busy or disconnected; try copying again"))
    }
}

pub enum Event {
    Listening(u16),
    Discovered {
        id: String,
        name: String,
        address: SocketAddr,
    },
    DiscoveryLost(String),
    ConnectionFailed {
        id: Option<String>,
        address: String,
        error: String,
    },
    Connected {
        id: String,
        key: String,
        code: String,
        session: Uuid,
        sender: PeerTx,
    },
    Message {
        id: String,
        session: Uuid,
        message: Message,
    },
    Disconnected {
        id: String,
        session: Uuid,
    },
    Warning(String),
}

async fn emit(bus: &Bus, event: Event) {
    let _ = bus.send(Command::Network(event)).await;
}

pub async fn start(config: Config, bus: Bus) -> Result<()> {
    let listener = TcpListener::bind(("0.0.0.0", 0))
        .await
        .context("Cannot open a local network listener")?;
    let port = listener.local_addr()?.port();
    emit(&bus, Event::Listening(port)).await;
    let id = config.id();
    let mdns = ServiceDaemon::new();
    let mut discovery = None;
    if let Ok(ref daemon) = mdns {
        let properties = [
            ("id", id.as_str()),
            ("name", config.name.as_str()),
            ("version", "1"),
        ];
        let service = ServiceInfo::new(
            SERVICE,
            &id[..16],
            &format!("duetclip-{}.local.", &id[..16]),
            "",
            port,
            &properties[..],
        )
        .map(|info| info.enable_addr_auto());
        if let Ok(info) = service {
            match daemon.register(info).and_then(|()| daemon.browse(SERVICE)) {
                Ok(receiver) => discovery = Some(receiver),
                Err(e) => {
                    emit(
                        &bus,
                        Event::Warning(format!(
                            "Discovery unavailable: {e}. You can connect by address."
                        )),
                    )
                    .await
                }
            }
        }
    } else {
        emit(
            &bus,
            Event::Warning("Network discovery is unavailable. Use Connect by address.".into()),
        )
        .await;
    }
    let discovery_bus = bus.clone();
    let own_id = id.clone();
    let discovery_task = tokio::spawn(async move {
        let mut services = HashMap::new();
        if let Some(receiver) = discovery {
            while let Ok(event) = receiver.recv_async().await {
                if let ServiceEvent::ServiceRemoved(_, fullname) = &event {
                    if let Some(id) = services.remove(fullname) {
                        emit(&discovery_bus, Event::DiscoveryLost(id)).await;
                    }
                }
                if let ServiceEvent::ServiceResolved(info) = event {
                    let peer = info.get_property_val_str("id").unwrap_or_default();
                    if peer == own_id || peer.len() != 64 {
                        continue;
                    }
                    services.insert(info.get_fullname().to_owned(), peer.to_owned());
                    let name = info
                        .get_property_val_str("name")
                        .unwrap_or("Nearby computer")
                        .chars()
                        .take(80)
                        .collect::<String>();
                    for address in info.get_addresses() {
                        let ip = address.to_ip_addr();
                        if ip.is_ipv4() {
                            emit(
                                &discovery_bus,
                                Event::Discovered {
                                    id: peer.into(),
                                    name: name.clone(),
                                    address: SocketAddr::new(ip, info.get_port()),
                                },
                            )
                            .await;
                        }
                    }
                }
            }
        }
    });
    let permits = std::sync::Arc::new(tokio::sync::Semaphore::new(16));
    while let Ok((stream, _address)) = listener.accept().await {
        let Ok(permit) = permits.clone().try_acquire_owned() else {
            continue;
        };
        let config = config.clone();
        let bus = bus.clone();
        tokio::spawn(async move {
            let _permit = permit;
            let _ = connection(stream, config, false, None, bus).await;
        });
    }
    discovery_task.abort();
    if let Ok(daemon) = mdns {
        let _ = daemon.shutdown();
    }
    Ok(())
}

pub fn connect(address: String, expected: Option<String>, config: Config, bus: Bus) {
    tokio::spawn(async move {
        let peer_id = expected.clone();
        let result = async {
            let stream = tokio::time::timeout(Duration::from_secs(5), TcpStream::connect(&address))
                .await??;
            connection(stream, config, true, expected, bus.clone()).await
        }
        .await;
        if let Err(error) = result {
            emit(
                &bus,
                Event::ConnectionFailed {
                    id: peer_id,
                    address,
                    error: error.to_string(),
                },
            )
            .await;
        }
    });
}

pub(crate) async fn connection(
    stream: TcpStream,
    config: Config,
    initiator: bool,
    expected: Option<String>,
    bus: Bus,
) -> Result<()> {
    let secure = tokio::time::timeout(
        Duration::from_secs(10),
        protocol::handshake(stream, &config, initiator),
    )
    .await??;
    if let Some(expected) = expected {
        anyhow::ensure!(
            secure.peer_id == expected,
            "Discovery identity does not match the encrypted connection"
        );
    }
    let id = secure.peer_id;
    let session = Uuid::new_v4();
    let (control, mut receiver) = mpsc::channel::<Message>(64);
    let (bulk, mut bulk_receiver) = mpsc::channel::<Message>(4);
    control
        .send(Message::Hello {
            name: config.name.chars().take(80).collect(),
            version: protocol::PROTOCOL_VERSION,
        })
        .await?;
    let sender = PeerTx { control, bulk };
    emit(
        &bus,
        Event::Connected {
            id: id.clone(),
            key: secure.peer_key,
            code: secure.code,
            session,
            sender,
        },
    )
    .await;
    let (mut reader, mut writer) = secure.stream.into_split();
    let read_cipher = secure.cipher.clone();
    let write_cipher = secure.cipher;
    let read_bus = bus.clone();
    let read_id = id.clone();
    let read = async move {
        loop {
            let frame =
                tokio::time::timeout(Duration::from_secs(45), protocol::read_frame(&mut reader))
                    .await??;
            let message = protocol::decrypt(&read_cipher, &frame)?;
            read_bus
                .send(Command::Network(Event::Message {
                    id: read_id.clone(),
                    session,
                    message,
                }))
                .await?;
        }
        #[allow(unreachable_code)]
        Ok::<(), anyhow::Error>(())
    };
    let write = async move {
        loop {
            let message = tokio::select! {
                biased;
                message = receiver.recv() => match message { Some(m) => m, None => break },
                message = bulk_receiver.recv() => match message { Some(m) => m, None => break },
            };
            let frame = protocol::encrypt(&write_cipher, &message)?;
            tokio::time::timeout(
                Duration::from_secs(30),
                protocol::write_frame(&mut writer, &frame),
            )
            .await??;
        }
        Ok::<(), anyhow::Error>(())
    };
    let result = tokio::select! { result = read => result, result = write => result };
    emit(&bus, Event::Disconnected { id, session }).await;
    if let Err(error) = result {
        tracing::debug!("Peer session ended: {error}");
    }
    Ok(())
}
