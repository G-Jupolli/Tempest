use bincode::{Decode, Encode};

#[derive(Debug, Encode, Decode, Clone, Copy)]
pub enum GameType {
    Uno,
}

#[derive(Debug, Encode, Decode, Clone, Copy, PartialEq, Eq)]
pub enum GameStartState {
    Setup,
    Active,
    Ending,
}

#[derive(Debug)]
pub enum GameUserState {
    Active,
    Disconnected,
    Left,
    Spectator,
}
