use bincode::{Decode, Encode};

#[derive(Debug)]
pub struct UserData {
    pub name: String,
    pub state: PlayerState,
}

#[derive(Debug)]
pub enum PlayerState {
    Lobby,
    Game(i64),
}

#[derive(Debug, Encode, Decode)]
pub enum UserCommand {
    Name(String),
    SelfId(u32),
}
