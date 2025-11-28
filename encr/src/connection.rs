use anyhow::{Context, Result};
use bincode::{Decode, Encode};
use futures::{SinkExt, StreamExt};
use snow::TransportState;
use std::marker::PhantomData;
use tokio::net::TcpStream;
use tokio::sync::{mpsc, oneshot};
use tokio_util::bytes::Bytes;
use tokio_util::codec::{Framed, LengthDelimitedCodec};

// TODO - change this name
// I hate it but I cant think of anything better right now
pub(crate) struct NoEncryptConnection {
    transport: TransportState,
    stream: TcpStream,
}

impl NoEncryptConnection {
    pub(crate) fn new(transport: TransportState, stream: TcpStream) -> Self {
        Self { transport, stream }
    }

    pub(crate) async fn send(
        framed: &mut Framed<TcpStream, LengthDelimitedCodec>,
        data: &[u8],
    ) -> Result<()> {
        framed
            .send(Bytes::copy_from_slice(data))
            .await
            .context("Failed to send handshake message")
    }

    pub(crate) async fn recv(
        framed: &mut Framed<TcpStream, LengthDelimitedCodec>,
    ) -> Result<Vec<u8>> {
        framed
            .next()
            .await
            .ok_or_else(|| anyhow::anyhow!("Connection closed during handshake"))?
            .context("Failed to receive handshake message")
            .map(|bytes| bytes.to_vec())
    }

    // This connection should not be used after the handshake is done.
    // We consume it to only use the encrypted connection going ahead.
    pub(crate) fn consume<T: Encode + Decode<()>>(
        self,
    ) -> (EncryptedSender<T>, EncryptedReceiver<T>) {
        let Self {
            mut transport,
            stream,
        } = self;

        let framed = Framed::new(stream, LengthDelimitedCodec::new());
        let (tcp_sender, tcp_receiver) = framed.split();

        let (encryption_sender, mut encryption_receiver) =
            mpsc::unbounded_channel::<EncryptionRequest>();

        // This was described to me but I'm 50/50 on if I like it.
        //
        // Using a oneshot channel allows lock free encrypt / decrypt handling.
        // I may have to manually compare doing this and a mutex on transport
        //   with some simulated data. Not sure which is better right now.
        tokio::spawn(async move {
            let mut buff = vec![0u8; 65535];

            while let Some(request) = encryption_receiver.recv().await {
                let result = match request.meta {
                    EncryptionRequestType::Encrypt => transport
                        .write_message(&request.raw_data, &mut buff)
                        .context("Encryption"),
                    EncryptionRequestType::Decrypt => transport
                        .read_message(&request.raw_data, &mut buff)
                        .context("Decryption"),
                }
                .map(|byte_len| buff[..byte_len].to_vec());

                let _ = request.responder.send(result).inspect_err(|err| {
                    println!("Failed to respond on encryption oneshot {err:?}");
                });
            }
        });

        (
            EncryptedSender {
                encryption_sender: encryption_sender.clone(),
                tcp_sender,
                marker: PhantomData,
            },
            EncryptedReceiver {
                encryption_sender,
                tcp_receiver,
                marker: PhantomData,
            },
        )
    }
}

struct EncryptionRequest {
    meta: EncryptionRequestType,
    raw_data: Vec<u8>,
    responder: oneshot::Sender<Result<Vec<u8>>>,
}

#[derive(Debug)]
enum EncryptionRequestType {
    Encrypt,
    Decrypt,
}

pub struct EncryptedSender<T: Encode> {
    encryption_sender: mpsc::UnboundedSender<EncryptionRequest>,
    tcp_sender: futures::stream::SplitSink<Framed<TcpStream, LengthDelimitedCodec>, Bytes>,
    marker: PhantomData<T>,
}

pub struct EncryptedReceiver<T: Decode<()>> {
    encryption_sender: mpsc::UnboundedSender<EncryptionRequest>,
    tcp_receiver: futures::stream::SplitStream<Framed<TcpStream, LengthDelimitedCodec>>,
    marker: PhantomData<T>,
}

impl<T: Encode> EncryptedSender<T> {
    pub async fn send(&mut self, msg: &T) -> Result<()> {
        let encoded = bincode::encode_to_vec(msg, bincode::config::standard())
            .context("Failed to encode message")?;

        let (response_tx, response_rx) = oneshot::channel();
        self.encryption_sender
            .send(EncryptionRequest {
                meta: EncryptionRequestType::Encrypt,
                raw_data: encoded,
                responder: response_tx,
            })
            .map_err(|_| anyhow::anyhow!("Encryption task closed"))?;

        let encrypted = response_rx
            .await
            .context("Encryption task dropped response")??;

        self.tcp_sender
            .send(Bytes::from(encrypted))
            .await
            .context("Failed to send encrypted message")?;

        Ok(())
    }
}

impl<T: Decode<()>> EncryptedReceiver<T> {
    pub async fn recv(&mut self) -> Result<T> {
        let encrypted = self
            .tcp_receiver
            .next()
            .await
            .ok_or_else(|| anyhow::anyhow!("Connection closed"))?
            .context("Failed to receive encrypted message")?
            .to_vec();

        let (response_tx, response_rx) = oneshot::channel();
        self.encryption_sender
            .send(EncryptionRequest {
                meta: EncryptionRequestType::Decrypt,
                raw_data: encrypted,
                responder: response_tx,
            })
            .map_err(|_| anyhow::anyhow!("Decryption task closed"))?;

        let raw_data = response_rx
            .await
            .context("Decryption task dropped response")??;

        let (msg, _) = bincode::decode_from_slice::<T, _>(&raw_data, bincode::config::standard())
            .context("Failed to decode message")?;

        Ok(msg)
    }
}
