use crate::connection::{EncryptedReceiver, EncryptedSender, NoEncryptConnection};
use anyhow::{Context, Result};
use bincode::{Decode, Encode};
use snow::Builder;
use std::sync::LazyLock;
use std::{marker::PhantomData, net::SocketAddr};
use tokio::net::{TcpListener, TcpStream};
use tokio_util::codec::{Framed, LengthDelimitedCodec};

static PARAMS: LazyLock<snow::params::NoiseParams> =
    LazyLock::new(|| "Noise_XX_25519_ChaChaPoly_BLAKE2s".parse().unwrap());

pub struct EncryptedServer<S: Encode, R: Decode<()>> {
    listener: TcpListener,
    static_key: Vec<u8>,
    _phantom_send: PhantomData<S>,
    _phantom_receive: PhantomData<R>,
}

pub struct ClientConnection<S: Encode, R: Decode<()>> {
    pub sender: EncryptedSender<S>,
    pub receiver: EncryptedReceiver<R>,
}

impl<S: Encode, R: Decode<()>> EncryptedServer<S, R> {
    pub async fn bind(addr: &str) -> Result<Self> {
        let listener = TcpListener::bind(addr)
            .await
            .context("Failed to bind server")?;

        let builder = Builder::new(PARAMS.clone());
        let static_key = builder.generate_keypair()?.private;

        Ok(Self {
            listener,
            static_key,
            _phantom_send: PhantomData,
            _phantom_receive: PhantomData,
        })
    }

    pub async fn accept(&self) -> Result<(ClientConnection<S, R>, SocketAddr)> {
        let (stream, addr) = self.listener.accept().await?;
        println!("New connection from: {}", addr);

        let connection = Self::perform_handshake(stream, &self.static_key).await?;
        let (sender, receiver) = connection.consume();

        Ok((ClientConnection { sender, receiver }, addr))
    }

    async fn perform_handshake(
        stream: TcpStream,
        static_key: &[u8],
    ) -> Result<NoEncryptConnection> {
        let mut framed = Framed::new(stream, LengthDelimitedCodec::new());

        // The example I saw does this for every connection.
        // I assume there is benefit to doing this instead of using the same
        // noise builder for all connections.
        let mut noise = Builder::new(PARAMS.clone())
            .local_private_key(static_key)?
            .build_responder()?;

        let mut buf = vec![0u8; 65535];

        // Need to start by receiving the client's ephemeral key
        // Todo - There should probably be a timeout here, if the client's
        //          connection disconnects, the recv will break
        //          but I need to handle leaked connection better.
        let msg = NoEncryptConnection::recv(&mut framed).await?;
        noise.read_message(&msg, &mut buf)?;

        // Responding with the server's ephemeral key
        let len = noise.write_message(&[], &mut buf)?;
        NoEncryptConnection::send(&mut framed, &buf[..len]).await?;

        let msg = NoEncryptConnection::recv(&mut framed).await?;
        noise.read_message(&msg, &mut buf)?;

        // Now that we have received the server client public key from the client
        // we can transition to transport mode for normal usage
        let transport = noise
            .into_transport_mode()
            .context("Failed to transition to transport mode")?;

        // Unwrap the framed stream back to TcpStream
        let stream = framed.into_inner();
        Ok(NoEncryptConnection::new(transport, stream))
    }
}
