use bincode::{Decode, Encode};

use crate::game_state::{GameStartState, GameType};

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
    // End,
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
