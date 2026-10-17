use crate::{config::Config, model::Offer};
use anyhow::{ensure, Context, Result};
use bincode::Options;
use serde::{Deserialize, Serialize};
use snow::{params::NoiseParams, TransportState};
use std::sync::{Arc, Mutex};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    net::TcpStream,
};
use uuid::Uuid;

pub const FRAME_LIMIT: usize = 65_535;
pub const CHUNK_SIZE: usize = 32_768;
pub const PROTOCOL_VERSION: u32 = 1;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Message {
    Hello {
        name: String,
        version: u32,
    },
    Trust,
    Offer(Offer),
    Request {
        id: Uuid,
        transfer: Uuid,
    },
    Chunk {
        id: Uuid,
        part: u32,
        offset: u64,
        data: Vec<u8>,
    },
    Complete {
        id: Uuid,
    },
    Cancel {
        id: Uuid,
    },
    Failed {
        id: Uuid,
        reason: String,
    },
    Ping,
}

pub fn noise_params() -> NoiseParams {
    "Noise_XX_25519_ChaChaPoly_BLAKE2s"
        .parse()
        .expect("constant protocol")
}

pub struct SecureSession {
    pub peer_key: String,
    pub peer_id: String,
    pub code: String,
    pub cipher: Arc<Mutex<TransportState>>,
    pub stream: TcpStream,
}

pub async fn handshake(
    mut stream: TcpStream,
    config: &Config,
    initiator: bool,
) -> Result<SecureSession> {
    stream.set_nodelay(true)?;
    let private = hex::decode(&config.private_key)?;
    let builder = snow::Builder::new(noise_params())
        .local_private_key(&private)?
        .prologue(b"duet-clipboard/v1")?;
    let mut noise = if initiator {
        builder.build_initiator()?
    } else {
        builder.build_responder()?
    };
    let mut buffer = vec![0u8; FRAME_LIMIT];
    if initiator {
        let n = noise.write_message(&[], &mut buffer)?;
        write_frame(&mut stream, &buffer[..n]).await?;
        noise.read_message(&read_frame(&mut stream).await?, &mut buffer)?;
        let n = noise.write_message(&[], &mut buffer)?;
        write_frame(&mut stream, &buffer[..n]).await?;
    } else {
        noise.read_message(&read_frame(&mut stream).await?, &mut buffer)?;
        let n = noise.write_message(&[], &mut buffer)?;
        write_frame(&mut stream, &buffer[..n]).await?;
        noise.read_message(&read_frame(&mut stream).await?, &mut buffer)?;
    }
    let key = noise
        .get_remote_static()
        .context("Peer did not provide an identity")?;
    let peer_key = hex::encode(key);
    let peer_id = blake3::hash(key).to_hex().to_string();
    ensure!(peer_id != config.id(), "This is the same device");
    // Both screens derive the same verification code from the entire handshake,
    // binding the comparison to this session and both permanent identities.
    let digest = blake3::hash(noise.get_handshake_hash())
        .to_hex()
        .to_string();
    let code = format!("{} {} {}", &digest[..4], &digest[4..8], &digest[8..12]);
    Ok(SecureSession {
        peer_key,
        peer_id,
        code,
        cipher: Arc::new(Mutex::new(noise.into_transport_mode()?)),
        stream,
    })
}

pub async fn write_frame<W: AsyncWrite + Unpin>(writer: &mut W, bytes: &[u8]) -> Result<()> {
    ensure!(
        !bytes.is_empty() && bytes.len() <= FRAME_LIMIT,
        "Invalid frame length"
    );
    writer.write_u32(bytes.len() as u32).await?;
    writer.write_all(bytes).await?;
    Ok(())
}

pub async fn read_frame<R: AsyncRead + Unpin>(reader: &mut R) -> Result<Vec<u8>> {
    let size = reader.read_u32().await? as usize;
    ensure!(size > 0 && size <= FRAME_LIMIT, "Invalid frame length");
    let mut bytes = vec![0u8; size];
    reader.read_exact(&mut bytes).await?;
    Ok(bytes)
}

pub fn encrypt(cipher: &Mutex<TransportState>, message: &Message) -> Result<Vec<u8>> {
    let plain = bincode::DefaultOptions::new()
        .with_limit((FRAME_LIMIT - 16) as u64)
        .serialize(message)?;
    let mut encrypted = vec![0u8; plain.len() + 16];
    let n = cipher
        .lock()
        .map_err(|_| anyhow::anyhow!("Session lock failed"))?
        .write_message(&plain, &mut encrypted)?;
    encrypted.truncate(n);
    Ok(encrypted)
}

pub fn decrypt(cipher: &Mutex<TransportState>, frame: &[u8]) -> Result<Message> {
    let mut plain = vec![0u8; frame.len()];
    let n = cipher
        .lock()
        .map_err(|_| anyhow::anyhow!("Session lock failed"))?
        .read_message(frame, &mut plain)?;
    Ok(bincode::DefaultOptions::new()
        .with_limit(FRAME_LIMIT as u64)
        .reject_trailing_bytes()
        .deserialize(&plain[..n])?)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn real_noise_session_authenticates_identity_and_rejects_tampering() {
        let a_dir = tempfile::tempdir().unwrap();
        let b_dir = tempfile::tempdir().unwrap();
        let a = Config::load(a_dir.path()).unwrap();
        let b = Config::load(b_dir.path()).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (left, right) = tokio::join!(
            async {
                handshake(TcpStream::connect(address).await.unwrap(), &a, true)
                    .await
                    .unwrap()
            },
            async {
                handshake(listener.accept().await.unwrap().0, &b, false)
                    .await
                    .unwrap()
            }
        );
        assert_eq!(left.peer_key, b.public_key);
        assert_eq!(right.peer_key, a.public_key);
        assert_eq!(left.code, right.code);
        let mut frame = encrypt(
            &left.cipher,
            &Message::Hello {
                name: "My laptop".into(),
                version: 1,
            },
        )
        .unwrap();
        assert!(!String::from_utf8_lossy(&frame).contains("My laptop"));
        frame[0] ^= 1;
        assert!(decrypt(&right.cipher, &frame).is_err());
        // An unauthentic packet does not consume the receive nonce.
        frame[0] ^= 1;
        assert!(matches!(
            decrypt(&right.cipher, &frame).unwrap(),
            Message::Hello { .. }
        ));
    }
    #[tokio::test]
    async fn rejects_oversized_frame_before_allocating() {
        let (mut a, mut b) = tokio::io::duplex(32);
        a.write_u32(1_000_000_000).await.unwrap();
        assert!(read_frame(&mut b).await.is_err());
    }
}
