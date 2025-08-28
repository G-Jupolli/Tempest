use anyhow::anyhow;
use color_eyre::{Result, eyre::Error};
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::DefaultTerminal;
use rpc::{
    comms::{ClientAuthedCommand, ClientMessage, ServerMessage, TcpReceiver, TcpSender},
    game_state::GameType,
};
use tokio::sync::mpsc;

use crate::{app_auth::AppAuth, app_lobby::LobbyResult, uno_client::UnoClient};

mod app_auth;
mod app_lobby;
mod uno_client;

/// This architecture may be a bit off, the main idea is:
///
/// We have 1 main thread that holds the terminal and listens to reads on a channel.
///
/// We have a 2nd thread that awaits for terminal events
/// A 3rd channel awaits for tcp messages we receive from the server
///
/// With the authentication model in use ( just submitting a name ) we need to
///   first submit the name then wait for the authentication details.
///
/// We can setup the terminal event listener thread instantly but
///   manually handle the authentication to ensure it's completion.
///   I'll look to splitting up the screens to separate folders,
///   moving connections when needed.
#[tokio::main]
async fn main() -> color_eyre::Result<()> {
    color_eyre::install()?;
    let terminal = ratatui::init();
    let result = App::start(terminal).await;
    ratatui::restore();
    result
}

#[derive(Debug)]
pub enum AppMessage {
    RpcEvent(ServerMessage),
    TerminalEvent(Event),
    Failure(anyhow::Error),
}

#[derive(Debug)]
pub enum TerminalState {
    Auth(String, String),
    Lobby,
}

#[derive(Debug)]
pub struct App {
    // running: bool,
    // state: TerminalState,
}

#[derive(Debug, Clone)]
pub enum GameResult {
    Exit,
    None,
    NoGame,
    Game(String, GameType),
}

impl App {
    // pub fn new() -> Self {
    //     App {
    //         // running: true,
    //         state: TerminalState::Auth("".to_string(), "".to_string()),
    //     }
    // }

    /// As above, here we want to start the channel to receive app events
    /// We can just handle the global exit case here and then pass events down
    pub async fn start(mut terminal: DefaultTerminal) -> Result<()> {
        let (app_sender, mut app_receiver) = mpsc::unbounded_channel::<AppMessage>();

        // Setup terminal event listener
        Self::start_read_thread(app_sender.clone());

        // Setup main event loop

        let (app_lobby, mut tcp_sender, tcp_receiver) =
            AppAuth::start_auth_loop(&mut terminal, &mut app_receiver)
                .await
                .map_err(|err| Error::msg(err))?;

        Self::start_rpc_receiver(app_sender.clone(), tcp_receiver);

        // let mut game_result = GameResult::None;

        loop {
            let lobby_result = app_lobby
                .clone()
                .start(&mut terminal, &mut app_receiver)
                .await
                .map_err(|err| Error::msg(err))?;

            if let LobbyResult::Exit = lobby_result {
                break;
            }

            let game_result = Self::handle_lobby_result(
                app_lobby.id,
                lobby_result,
                &mut tcp_sender,
                &mut app_receiver,
            )
            .await
            .map_err(|err| Error::msg(err))?;

            match game_result {
                GameResult::Exit => break,
                GameResult::None | GameResult::NoGame => continue,
                GameResult::Game(lobby, game_type) => match game_type {
                    GameType::Uno => {
                        UnoClient::try_start(
                            lobby,
                            app_lobby.name.clone(),
                            app_lobby.id,
                            &mut tcp_sender,
                            &mut app_receiver,
                            &mut terminal,
                        )
                        .await
                        .map_err(|err| Error::msg(err))?;
                    }
                },
            }
        }

        Ok(())
    }

    async fn handle_lobby_result(
        user_id: u32,
        lobby_result: LobbyResult,
        tcp_sender: &mut TcpSender<ClientMessage>,
        app_receiver: &mut mpsc::UnboundedReceiver<AppMessage>,
    ) -> anyhow::Result<GameResult> {
        match lobby_result {
            app_lobby::LobbyResult::Exit => return Ok(GameResult::Exit),
            app_lobby::LobbyResult::Create(game_create) => {
                tcp_sender
                    .send(&ClientMessage::Authed(
                        user_id,
                        ClientAuthedCommand::CreateGame(
                            String::from_iter(game_create.name),
                            game_create.game_type,
                        ),
                    ))
                    .await?;
            }
            app_lobby::LobbyResult::Join(game_id) => {
                tcp_sender
                    .send(&ClientMessage::Authed(
                        user_id,
                        ClientAuthedCommand::JoinGame(game_id),
                    ))
                    .await?;
            }
        }

        loop {
            let Some(msg) = app_receiver.recv().await else {
                println!("App Receiver Down");
                return Ok(GameResult::Exit);
            };

            match msg {
                AppMessage::RpcEvent(server_message) => match server_message {
                    ServerMessage::AuthResponse(_)
                    | ServerMessage::LobbyState(_)
                    | ServerMessage::GameState(_)
                    | ServerMessage::NewPlayerCount(_) => {
                        return Err(anyhow!(
                            "Received redundant server message {server_message:?}"
                        ));
                    }
                    ServerMessage::JoinedGame(lobby, game_type) => {
                        return Ok(GameResult::Game(lobby, game_type));
                    }
                },
                AppMessage::TerminalEvent(event) => {
                    if let Event::Key(key_event) = event
                        && key_event.code == KeyCode::Esc
                        && key_event.kind == KeyEventKind::Release
                    {
                        return Ok(GameResult::Exit);
                    }
                }
                AppMessage::Failure(err) => {
                    return Err(err);
                }
            }
        }
    }

    fn start_rpc_receiver(
        event_submitter: mpsc::UnboundedSender<AppMessage>,
        mut receiver: TcpReceiver<ServerMessage>,
    ) {
        tokio::spawn(async move {
            while let Some(msg) = receiver.next_message().await {
                let msg = match msg {
                    Ok((msg, _)) => msg,
                    Err(err) => {
                        println!("{err:?}");
                        break;
                    }
                };

                let _ = event_submitter
                    .send(AppMessage::RpcEvent(msg))
                    .inspect_err(|err| {
                        println!(
                            "CRITICAL FAILURE, unable to send to main message loop from RPC {err:?}"
                        )
                    });
            }
        });
    }

    /// The choice to use a new thread here is intentional.
    /// Crossterm's event reader is sync and we need to not be blocking
    ///  the tcpStream from receiving messages by the event reader.
    ///  There is the poll method I can use to kind of make this async
    ///  but that was not very easy to work with in the past and messages
    ///  got mixed in some unideal ways
    fn start_read_thread(event_submitter: mpsc::UnboundedSender<AppMessage>) {
        std::thread::spawn(move || {
            loop {
                let msg = match event::read() {
                    Ok(ev) => AppMessage::TerminalEvent(ev),
                    Err(err) => {
                        AppMessage::Failure(anyhow!("Terminal event loop failed").context(err))
                    }
                };

                // This is a bit of an odd one to handle, the only time this fails is if the read
                //  on the channel is dropped, and that happens when the process exits so this
                //  really should never happen, just have this print here just in case.
                let _ = event_submitter.send(msg).inspect_err(|err| {
                    println!("CRITICAL FAILURE, unable to send to main message loop {err:?}")
                });
            }
        });
    }
}
