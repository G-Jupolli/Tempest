use bincode::{Decode, Encode};

pub mod command;
pub mod game_state;
pub mod user_state;

pub mod comms;

pub mod uno;

#[derive(Debug, Encode, Decode, Clone)]
pub struct NamedUser(u32, String);
