use std::marker::PhantomData;

use anyhow::anyhow;
use bincode::{Decode, Encode, config::Configuration};
use futures::{
    SinkExt, StreamExt,
    stream::{SplitSink, SplitStream},
};
use tokio::net::TcpStream;
use tokio_util::{
    bytes::Bytes,
    codec::{Framed, LengthDelimitedCodec},
};

use crate::game_state::{GameStartState, GameType};

pub type FramedStream = Framed<TcpStream, LengthDelimitedCodec>;

/// This denotes that we must send only on type E
///
/// Clients should send client commands and receive server
///  commands, this is how we don't mix them
pub struct TcpSender<E: Encode> {
    stream: SplitSink<FramedStream, Bytes>,
    marker: PhantomData<E>,
}
pub struct TcpReceiver<D: Decode<()>> {
    stream: SplitStream<FramedStream>,
    marker: PhantomData<D>,
}

#[derive(Debug, Encode, Decode)]
pub enum ClientMessage {
    Authenticate(String),
    Authed(u32, ClientAuthedCommand),
}

#[derive(Debug, Encode, Decode)]
pub enum ClientAuthedCommand {
    CreateGame(String, GameType),
    Game(ClientGameCommand),
    JoinGame(u32),
}

#[derive(Debug, Encode, Decode)]
pub enum ClientGameCommand {
    Start,
    End,
    Leave,
    Raw(Vec<u8>),
}

#[derive(Debug, Encode, Decode)]
pub enum ServerMessage {
    AuthResponse(u32),
    LobbyState(ClientLobbyState),
    NewPlayerCount(usize),
    JoinedGame(String, GameType),
    GameState(Vec<u8>),
}

#[derive(Debug, Encode, Decode, Default, Clone)]
pub struct ClientLobbyState {
    pub player_count: usize,
    pub games: Vec<LobbyGame>,
}

#[derive(Debug, Encode, Decode, Clone)]
pub struct LobbyGame {
    pub name: String,
    pub id: u32,
    pub game_type: GameType,
    pub start_state: GameStartState,
    pub active_players: u32,
}

pub enum ReceiverFailure {
    ConnectionClose,
    Failed(anyhow::Error),
}

/// Structured as:
///
/// E -> Sending Type
///
/// D -> Receiving Type
pub fn split_stream<E: Encode, D: Decode<()>>(
    stream: tokio::net::TcpStream,
) -> (TcpSender<E>, TcpReceiver<D>) {
    let (send, recv) = Framed::new(stream, LengthDelimitedCodec::new()).split();

    (
        TcpSender {
            stream: send,
            marker: PhantomData,
        },
        TcpReceiver {
            stream: recv,
            marker: PhantomData,
        },
    )
}

impl<E: Encode> TcpSender<E> {
    pub async fn send(&mut self, msg: &E) -> anyhow::Result<()> {
        let bytes = bincode::encode_to_vec(msg, bincode::config::standard())?;

        let message_res = self.stream.send(bytes.into()).await;

        message_res?;

        Ok(())
    }
}

impl<D: Decode<()>> TcpReceiver<D> {
    pub async fn next_message(&mut self) -> Option<anyhow::Result<(D, usize)>> {
        if let Some(msg) = self.stream.next().await {
            let bytes = match msg {
                Ok(b) => b,
                Err(err) => {
                    return Some(Err(anyhow!("Error from Stream").context(err)));
                }
            };

            return match bincode::decode_from_slice::<D, Configuration>(
                &bytes,
                bincode::config::standard(),
            ) {
                Ok((msg, size)) => Some(Ok((msg, size))),
                Err(err) => Some(Err(anyhow!("Failed to decode").context(err))),
            };
        }

        None
    }
}
