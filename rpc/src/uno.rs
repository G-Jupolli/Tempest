use bincode::{Decode, Encode};

use crate::game_state::GameStartState;

/// Let's consider an uno card.
/// There are 3 parts to what can happen in a card.
/// Colour ~ Reb, Blue, Green, Yellow & Power
/// Value  ~ 0-9 for colour, or Power Action
/// Colour select on specific
///
/// If we want to break this into bits, we can have
///
/// 1 Bit to dictate power
/// 3 Bits to dictate colour
/// 5 bits for value, Really one is junk but it doesn't matter
///
/// In doing this, we see if it is a power card first
/// Then if such a power card controls colour, we use the colour section
///   to dictate the switch
///
/// In doing this, we can fit the card action into one byte, we can also store this
///   against the player for validation
///
/// Bit layout:
/// [power:1][color:2][value:5]
#[derive(PartialEq, Eq, Debug, Copy, Clone, Encode, Decode, PartialOrd, Ord)]
pub struct UnoCard(pub u8);

pub enum UnoCommand {}

pub enum PlayerUnoCommand {
    PlayCard(u8),
    PickupCard,
}

#[derive(Debug, Encode, Decode, Clone)]
pub struct UnoClientGameState {
    pub game_state: GameStartState,
    pub action: Vec<UnoAction>,
    pub finished_users: Vec<(u32, String)>,
    pub bust_users: Vec<(u32, String)>,
    pub active_users: Vec<UnoActiveUser>,
    pub host_user: u32,
    pub user_turn: u8,
    pub is_ord: bool,
    pub last_card: UnoCard,
}

#[derive(Debug, Encode, Decode, Clone)]
pub enum UnoAction {
    Init,
    InitialCard(UnoCard),
    UserPlaceCard(String, UnoCard),
    UserPickup(String, u8),
    UserJoined(String),
    UserLeft(String),
    UserFinished(String),
    UserBust(String),
    GameEnded,
}

#[derive(Debug, Encode, Decode, Clone)]
pub struct UnoActiveUser {
    pub id: u32,
    pub name: String,
    pub card_count: u32,
}

#[derive(Debug, Encode, Decode)]
pub enum ServerUnoCommand {
    // PlayerJoined(NamedUser),
    // PlayerSpectating(NamedUser),
    // PlayerCardPickup,
    // CardPlayed(UnoCard),
    GameState(Vec<UnoCard>, UnoClientGameState),
}

#[derive(Debug, Encode, Decode)]
pub enum UnoClientAction {
    PickupCard,
    PlayCard(UnoCard),
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnoCardColour {
    Red = 0,
    Blue = 1,
    Green = 2,
    Yellow = 3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum UnoCardPower {
    PlusTwo = 0,
    Skip = 1,
    Reverse = 2,
    PlusFour = 3,
    ClrChange = 4,
}

impl From<u8> for UnoCardColour {
    fn from(raw: u8) -> Self {
        match raw {
            0 => UnoCardColour::Red,
            1 => UnoCardColour::Blue,
            2 => UnoCardColour::Green,
            3 => UnoCardColour::Yellow,
            _ => {
                panic!("Unreachable panic ( pos_to_card )");
            }
        }
    }
}

impl From<u8> for UnoCardPower {
    fn from(raw: u8) -> Self {
        match raw {
            0 => UnoCardPower::PlusTwo,
            1 => UnoCardPower::Skip,
            2 => UnoCardPower::Reverse,
            3 => UnoCardPower::PlusFour,
            4 => UnoCardPower::ClrChange,
            _ => {
                panic!("Unreachable panic ( pos_to_card )");
            }
        }
    }
}

impl UnoCard {
    pub fn encode(power: bool, clr: UnoCardColour, value: u8) -> UnoCard {
        if value > 9 || (power && value > 4) {
            panic!("Tried to encode card for value too high")
        }

        let mut card: u8 = (clr as u8) << 5 | value;

        if power {
            card |= 0b10000000;
        }

        UnoCard(card)
    }

    pub fn decode(self) -> (bool, UnoCardColour, u8) {
        (
            self.0 & 0b10000000 != 0,
            UnoCardColour::from((self.0 & 0b01100000) >> 5),
            self.0 & 0b00011111,
        )
    }

    pub fn validate(self) -> bool {
        if self.0 & 0b10000000 == 0 {
            self.0 & 0b00011111 <= 9
        } else {
            self.0 & 0b00011111 <= 4
        }
    }

    // Plus 4    : 0b10000011
    // Clr Change: 0b10000100
    pub fn is_black(self) -> bool {
        matches!(self.0 & 0b10000111, 0b10000011 | 0b10000100)
    }

    pub fn is_power(self) -> bool {
        self.0 & 0b10000000 != 0
    }

    pub fn get_value(self) -> u8 {
        self.0 & 0b00011111
    }

    pub fn and(self, cmp: u8) -> u8 {
        self.0 & cmp
    }
}
