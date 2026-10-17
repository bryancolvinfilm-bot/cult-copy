use crate::engine::{Command, Handle};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    io::Write,
    net::{Ipv4Addr, SocketAddr, TcpStream},
    path::Path,
    time::Duration,
};
use tokio::io::AsyncReadExt;
use uuid::Uuid;

#[derive(Serialize, Deserialize)]
struct Endpoint {
    port: u16,
    token: Uuid,
}

pub fn activate(dir: &Path) -> Result<()> {
    let endpoint: Endpoint = serde_json::from_slice(&std::fs::read(dir.join("instance.json"))?)?;
    let mut stream = TcpStream::connect_timeout(
        &SocketAddr::from((Ipv4Addr::LOCALHOST, endpoint.port)),
        Duration::from_secs(1),
    )
    .context("The app is starting; open it again in a moment")?;
    stream.set_write_timeout(Some(Duration::from_secs(1)))?;
    stream.write_all(endpoint.token.as_bytes())?;
    Ok(())
}

pub fn listen(dir: &Path, handle: Handle, runtime: &tokio::runtime::Runtime) -> Result<()> {
    let listener = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
    listener.set_nonblocking(true)?;
    let endpoint = Endpoint {
        port: listener.local_addr()?.port(),
        token: Uuid::new_v4(),
    };
    std::fs::write(dir.join("instance.json"), serde_json::to_vec(&endpoint)?)?;
    runtime.spawn(async move {
        let Ok(listener) = tokio::net::TcpListener::from_std(listener) else {
            return;
        };
        while let Ok((mut stream, _)) = listener.accept().await {
            let mut token = [0u8; 16];
            if matches!(
                tokio::time::timeout(Duration::from_millis(500), stream.read_exact(&mut token))
                    .await,
                Ok(Ok(_))
            ) && token == *endpoint.token.as_bytes()
            {
                let _ = handle.bus.send(Command::Open).await;
            }
        }
    });
    Ok(())
}
