use crate::{connection_receiver::ConnectionReceiver, server_uno::ServerUno};
use rpc::{
    comms::{ClientAuthedCommand, ClientGameCommand, ClientLobbyState, LobbyGame, ServerMessage},
    game_state::{GameStartState, GameType},
    uno::UnoCard,
};
use std::{collections::HashMap, net::SocketAddr};
use tokio::sync::mpsc::{self, UnboundedSender};

mod connection_receiver;
mod server_uno;

struct TempestServer;

#[derive(Debug)]
pub enum ServerIntraMessage {
    RegisterUser(String, SocketAddr, UnboundedSender<ServerMessage>),
    Auth(AuthIntraMessage),
    UpdateUserLobbies,
    Disconnected(SocketAddr),
    UpdateGameServer(u32, GameServerStateUpdate),
    UserJoinedGame(u32, u32),
}

#[derive(Debug)]
pub struct AuthIntraMessage {
    addr: SocketAddr,
    user_id: u32,
    message: ClientAuthedCommand,
}

#[derive(Debug, Clone)]
pub struct PlayerState {
    pub name: String,
    pub addr: SocketAddr,
    pub sender: UnboundedSender<ServerMessage>,
    pub game_id: Option<u32>,
}

#[derive(Debug)]
pub struct GameServerState {
    pub name: String,
    pub player_count: u32,
    pub game_type: GameType,
    pub channel: UnboundedSender<GameServerMessage>,
    pub start_state: GameStartState,
}

/// Maybe switch this out and not store the channel
/// Here but in a tuple
#[derive(Debug)]
pub struct GameServerStateUpdate {
    pub name: String,
    pub player_count: u32,
    pub game_type: GameType,
    pub start_state: GameStartState,
}

#[derive(Debug)]
pub struct GameServerMessage {
    pub user_id: u32,
    pub command: ServerGameCommand,
}

#[derive(Debug)]
pub enum ServerGameCommand {
    // Start,
    // End,
    UserJoin(PlayerState),
    Cmd(ClientGameCommand),
}

pub struct InGameUser {
    id: u32,
    name: String,
    sender: UnboundedSender<ServerMessage>,
}

impl TempestServer {
    // We need to setup any internal connections and start the main listener for incoming connections
    pub async fn start_server() {
        let mut users: HashMap<u32, PlayerState> = HashMap::new();
        let mut games: HashMap<u32, GameServerState> = HashMap::new();

        // Seeing as this is an event loop, this is just enforcing a level of
        //  uniqueness without bothering with crypto.
        // A real server would need something better but this is good enough
        //  for this use case.
        let mut last_id: u32 = 0;

        let (event_sender, mut event_receiver) = mpsc::unbounded_channel::<ServerIntraMessage>();

        {
            let event_sender = event_sender.clone();

            tokio::spawn(async move { ConnectionReceiver::start_listener(event_sender).await });
        }

        while let Some(msg) = event_receiver.recv().await {
            println!("SPINNING {msg:?}");

            match msg {
                ServerIntraMessage::RegisterUser(name, addr, sender) => {
                    last_id += 1;
                    let id = last_id;

                    let _ = sender.send(ServerMessage::AuthResponse(id));
                    users.insert(
                        id,
                        PlayerState {
                            name,
                            addr,
                            sender,
                            game_id: None,
                        },
                    );

                    let _ = event_sender.send(ServerIntraMessage::UpdateUserLobbies);
                }
                ServerIntraMessage::Auth(msg) => {
                    println!("Received Auth Message {msg:?}");

                    if let Some(user) = users.get_mut(&msg.user_id) {
                        if user.addr != msg.addr {
                            println!(
                                "Received message for user {:?} on wrong addr {:?}",
                                user, msg
                            );
                            continue;
                        }

                        println!("Found user {user:?}");

                        match msg.message {
                            ClientAuthedCommand::CreateGame(lobby_name, game_type) => {
                                if user.game_id.is_some() {
                                    println!("!!! >> User created Game when in game");
                                }

                                println!("Now Create New Game {lobby_name} -> {game_type:?}");
                                last_id += 1;
                                let game_id = last_id;

                                // I really need to fix these switch cases.
                                // These big sections should be moved to their own functions
                                // Having 10 indentations is a bit crazy
                                let server = match game_type {
                                    GameType::Uno => ServerUno::create(
                                        game_id,
                                        msg.user_id,
                                        user,
                                        lobby_name,
                                        event_sender.clone(),
                                    ),
                                };

                                match server {
                                    Ok(server) => {
                                        println!("Have created some Game Server {server:?}");

                                        user.game_id = Some(game_id);
                                        games.insert(game_id, server);

                                        let _ = event_sender
                                            .send(ServerIntraMessage::UpdateUserLobbies);
                                    }
                                    Err(err) => {
                                        println!("Failed to create Game Server {err:?}");
                                        todo!("Handle Game Server Creation Fail")
                                    }
                                }
                            }
                            ClientAuthedCommand::Game(command) => {
                                let Some(game_id) = user.game_id else {
                                    println!("User send game command without being in game");
                                    continue;
                                };

                                let Some(game) = games.get(&game_id) else {
                                    println!("User send game command in game, but game not found");
                                    continue;
                                };

                                let _ = game
                                    .channel
                                    .send(GameServerMessage {
                                        user_id: msg.user_id,
                                        command: ServerGameCommand::Cmd(command),
                                    })
                                    .inspect_err(|err| {
                                        println!(
                                            "Failed to send to game channel {} {err:?}",
                                            game_id
                                        );
                                    });
                            }
                            ClientAuthedCommand::JoinGame(game_id) => {
                                if user.game_id != None {
                                    println!(
                                        "User tried to Join a game when already in a game {} -> {} : {:?}",
                                        msg.user_id, game_id, user.game_id
                                    );
                                    continue;
                                }

                                let Some(game) = games.get(&game_id) else {
                                    println!(
                                        "User tired to join a non existing game {} -> {}",
                                        msg.user_id, game_id
                                    );
                                    continue;
                                };

                                let _x = game.channel.send(GameServerMessage {
                                    user_id: msg.user_id,
                                    command: ServerGameCommand::UserJoin(user.clone()),
                                });
                            }
                        }
                    }
                }
                ServerIntraMessage::UpdateUserLobbies => {
                    let lobby_state = ClientLobbyState {
                        player_count: users.len(),
                        games: games
                            .iter()
                            .filter(|(_, game)| {
                                game.start_state == GameStartState::Setup && game.player_count < 4
                            })
                            .map(|(game_id, game)| LobbyGame {
                                name: game.name.clone(),
                                id: *game_id,
                                game_type: game.game_type,
                                start_state: game.start_state,
                                active_players: game.player_count,
                            })
                            .collect(),
                    };

                    for (_, state) in users.iter() {
                        if state.game_id.is_none() {
                            let _ = state
                                .sender
                                .send(ServerMessage::LobbyState(lobby_state.clone()));
                        }
                    }
                }
                ServerIntraMessage::Disconnected(socket_addr) => {
                    users.retain(|_, user| {
                        if user.addr == socket_addr {
                            println!("Disconnect User {:?}", user);
                            false
                        } else {
                            true
                        }
                    });
                }
                ServerIntraMessage::UpdateGameServer(id, updated) => {
                    let Some(game) = games.get_mut(&id) else {
                        println!("Tried to send update for game not existing {id}");
                        continue;
                    };

                    game.name = updated.name;
                    game.player_count = updated.player_count;
                    game.start_state = updated.start_state;

                    let _ = event_sender.send(ServerIntraMessage::UpdateUserLobbies);
                }
                ServerIntraMessage::UserJoinedGame(user_id, game_id) => {
                    if let Some(user) = users.get_mut(&user_id) {
                        user.game_id = Some(game_id);
                    } else {
                        println!(
                            "Received ServerIntraMessage::UserJoinedGame for non existing user {user_id}"
                        );
                    }
                }
            }
        }
    }
}

#[tokio::main]
async fn main() {
    // let card = UnoCard(163);

    // let d = card.decode();

    // println!("Card {:?}", d);

    // println!("{:?}", card.is_black());
    TempestServer::start_server().await;
}
