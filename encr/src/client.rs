use crate::connection::{EncryptedReceiver, EncryptedSender, NoEncryptConnection};
use anyhow::{Context, Result};
use bincode::{Decode, Encode};
use snow::Builder;
use std::sync::LazyLock;
use tokio::net::TcpStream;
use tokio_util::codec::{Framed, LengthDelimitedCodec};

static PARAMS: LazyLock<snow::params::NoiseParams> =
    LazyLock::new(|| "Noise_XX_25519_ChaChaPoly_BLAKE2s".parse().unwrap());

pub struct EncryptedClient<S: Encode, R: Decode<()>> {
    pub sender: EncryptedSender<S>,
    pub receiver: EncryptedReceiver<R>,
}

impl<S: Encode, R: Decode<()>> EncryptedClient<S, R> {
    pub async fn connect(addr: &str) -> Result<Self> {
        let stream = TcpStream::connect(addr)
            .await
            .context("Failed to connect to server")?;

        let mut framed = Framed::new(stream, LengthDelimitedCodec::new());

        let builder = Builder::new(PARAMS.clone());
        let static_key = builder.generate_keypair()?.private;
        let mut noise = builder.local_private_key(&static_key)?.build_initiator()?;

        let mut buf = vec![0u8; 65535];

        let len = noise.write_message(&[], &mut buf)?;
        NoEncryptConnection::send(&mut framed, &buf[..len]).await?;

        let msg = NoEncryptConnection::recv(&mut framed).await?;
        noise.read_message(&msg, &mut buf)?;

        let len = noise.write_message(&[], &mut buf)?;
        NoEncryptConnection::send(&mut framed, &buf[..len]).await?;

        let transport = noise
            .into_transport_mode()
            .context("Failed to transition to transport mode")?;

        let stream = framed.into_inner();
        let connection = NoEncryptConnection::new(transport, stream);
        let (sender, receiver) = connection.consume();

        Ok(Self { sender, receiver })
    }
}
