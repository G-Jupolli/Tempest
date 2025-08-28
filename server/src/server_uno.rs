use std::collections::HashMap;

use anyhow::anyhow;
use bincode::config::Configuration;
use rand::Rng;
use rpc::{
    comms::{ClientGameCommand, ServerMessage},
    game_state::{GameStartState, GameType},
    uno::{
        ServerUnoCommand, UnoAction, UnoActiveUser, UnoCard, UnoCardColour, UnoCardPower,
        UnoClientAction, UnoClientGameState,
    },
};
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};

use crate::{
    GameServerMessage, GameServerState, GameServerStateUpdate, PlayerState, ServerGameCommand,
    ServerIntraMessage,
};

/// My idea for game implementations is that they are stored
///  not in full data, but via a channel.
///
/// In doing this, a given game can run on it's own thread
///  and just wait to receive game specific messages.
///
/// I think I'll have game data level messages to be decoded
///  on the game's thread itself to not over complicate the
///  decode on the main loop
pub struct ServerUno {
    id: u32,
    lobby_name: String,
    deck: UnoDeck,
    active_users: Vec<UnoUser>,
    finished_users: Vec<(u32, String)>,
    bust_users: Vec<(u32, String)>,
    user_senders: HashMap<u32, UnboundedSender<ServerMessage>>,
    last_card: UnoCard,
    host_user: u32,
    user_turn: u8,
    is_ord: bool,
    pub start_state: GameStartState,
    action: Vec<UnoAction>,
}

#[derive(Debug)]
struct UnoUser {
    id: u32,
    name: String,
    cards: Vec<UnoCard>,
}

// struct WaitingUser {
//     id: u32,
//     name: String,
//     sender: UnboundedSender<ServerMessage>,
// }

const DECK_SIZE: u8 = 108;

impl ServerUno {
    /// The idea here is to create the game on it's own thread
    ///  we can then return a channel to the game thread for
    ///  the main loop to send messages to.
    /// We also need a channel to the main thread here.
    pub fn create(
        game_id: u32,
        host_id: u32,
        host: &PlayerState,
        lobby_name: String,
        service_sender: UnboundedSender<ServerIntraMessage>,
    ) -> anyhow::Result<GameServerState> {
        let (send_channel, receive_channel) = mpsc::unbounded_channel::<GameServerMessage>();

        let mut deck = UnoDeck::new();
        let mut rng = rand::rng();

        let last_card = deck.pickup();

        let host_player = UnoUser {
            id: host_id,
            name: host.name.clone(),
            cards: deck.get_new_hand(&mut rng)?,
        };

        let mut user_senders = HashMap::new();
        user_senders.insert(host_id, host.sender.clone());

        let state = GameServerState {
            name: lobby_name.clone(),
            player_count: 1,
            game_type: GameType::Uno,
            channel: send_channel,
            start_state: GameStartState::Setup,
        };

        let _x = host
            .sender
            .send(ServerMessage::JoinedGame(lobby_name.clone(), GameType::Uno));

        tokio::spawn(async move {
            let server = ServerUno {
                id: game_id,
                lobby_name,
                deck,
                active_users: vec![host_player],
                finished_users: vec![],
                bust_users: vec![],
                last_card,
                host_user: host_id,
                user_turn: 0,
                is_ord: true,
                start_state: GameStartState::Setup,
                action: vec![UnoAction::Init],
                user_senders,
            };

            server.start_server(receive_channel, service_sender).await;
        });

        Ok(state)
    }

    async fn start_server(
        mut self,
        mut receiver_channel: UnboundedReceiver<GameServerMessage>,
        service_sender: UnboundedSender<ServerIntraMessage>,
    ) {
        self.update_user_state();

        while let Some(msg) = receiver_channel.recv().await {
            let cmd = match msg.command {
                ServerGameCommand::UserJoin(user) => {
                    if self.active_users.len() >= 4 || self.start_state != GameStartState::Setup {
                        println!("Not allowed in");
                        continue;
                    }

                    let (user, user_sender) =
                        UnoUser::new_joiner(&mut self.deck, msg.user_id, user);

                    let _x = user_sender.send(ServerMessage::JoinedGame(
                        self.lobby_name.clone(),
                        GameType::Uno,
                    ));

                    self.user_senders.insert(user.id, user_sender);
                    self.action.push(UnoAction::UserJoined(user.name.clone()));
                    self.active_users.push(user);

                    self.update_user_state();

                    let _x = service_sender
                        .send(ServerIntraMessage::UserJoinedGame(msg.user_id, self.id));

                    let _x = service_sender.send(self.service_update_state());

                    continue;
                }
                ServerGameCommand::Cmd(cmd) => cmd,
            };

            match cmd {
                ClientGameCommand::Start => {
                    if msg.user_id != self.host_user || self.start_state != GameStartState::Setup {
                        println!("Pointless start message from {}", msg.user_id);
                        continue;
                    }
                    if self.active_users.len() < 2 {
                        println!("Tried to start a game with less than 2 people");
                        continue;
                    }
                    self.start_state = GameStartState::Active;
                    let _x = service_sender.send(self.service_update_state());
                    self.update_user_state();
                }
                ClientGameCommand::Raw(raw_data) => {
                    let Some(user_idx) = self
                        .active_users
                        .iter()
                        .position(|user| user.id == msg.user_id)
                    else {
                        println!("Received message for user not in game {}", msg.user_id);
                        continue;
                    };

                    let user_idx = user_idx as u8;

                    let Ok((action, _)) =
                        bincode::decode_from_slice::<UnoClientAction, Configuration>(
                            &raw_data,
                            bincode::config::standard(),
                        )
                        .inspect_err(|err| {
                            println!(
                                "Failed to decode uno action for user {} : {err:?}",
                                msg.user_id
                            );
                        })
                    else {
                        continue;
                    };

                    if self.start_state != GameStartState::Active {
                        println!("Received Game message when not active");
                        continue;
                    }

                    match action {
                        UnoClientAction::PickupCard => {
                            if self.user_turn != user_idx {
                                println!(
                                    "Received message from user when not turn {} : {action:?}",
                                    msg.user_id
                                );
                            }

                            let card = self.deck.pickup();
                            let user = self
                                .active_users
                                .get_mut(user_idx as usize)
                                .expect("User should be valid here from position get");

                            self.action
                                .push(UnoAction::UserPickup(user.name.clone(), 1));

                            user.cards.push(card);
                            self.push_turn();

                            self.update_user_state();
                        }
                        UnoClientAction::PlayCard(uno_card) => {
                            let cards_left = match self.submit_card(msg.user_id, uno_card) {
                                Ok(cards_left) => cards_left,
                                Err(err) => {
                                    println!(
                                        "Not allowed to submit such card {} -> {uno_card:?} : {err:?}",
                                        msg.user_id
                                    );
                                    continue;
                                }
                            };
                            self.commit_card(uno_card);

                            if cards_left == 0 {
                                self.user_finished(msg.user_id);
                            }
                            self.check_user_bust();

                            self.update_user_state();
                        }
                    }
                }
                ClientGameCommand::Leave => {
                    if let Some(user_idx) = self
                        .active_users
                        .iter()
                        .position(|user| user.id == msg.user_id)
                    {
                        let user = self.active_users.remove(user_idx);

                        self.action.push(UnoAction::UserLeft(user.name.clone()));

                        if self.start_state != GameStartState::Setup {
                            self.bust_users.push((user.id, user.name));
                        }
                    } else if let Some((_, user)) = self
                        .finished_users
                        .iter()
                        .find(|(u_id, _)| msg.user_id.eq(u_id))
                    {
                        self.action.push(UnoAction::UserLeft(user.clone()));
                    } else if let Some((_, user)) = self
                        .bust_users
                        .iter()
                        .find(|(u_id, _)| msg.user_id.eq(u_id))
                    {
                        self.action.push(UnoAction::UserLeft(user.clone()));
                    };

                    let _ = self.user_senders.remove(&msg.user_id);
                    self.check_over();

                    let _x =
                        service_sender.send(ServerIntraMessage::UserLeftGame(msg.user_id, self.id));
                    let _x = service_sender.send(self.service_update_state());
                }
            }

            // Nobody is in the server at this point
            if self.user_senders.is_empty() {
                break;
            }
        }

        let _ = service_sender.send(ServerIntraMessage::GameFinished(self.id));
    }

    fn service_update_state(&self) -> ServerIntraMessage {
        ServerIntraMessage::UpdateGameServer(
            self.id,
            GameServerStateUpdate {
                name: self.lobby_name.clone(),
                player_count: self.active_users.len() as u32,
                game_type: GameType::Uno,
                start_state: self.start_state,
            },
        )
    }

    fn update_user_state(&mut self) {
        let state = UnoClientGameState {
            game_state: self.start_state,
            action: self.action.drain(..).collect(),
            active_users: self
                .active_users
                .iter()
                .map(|user| UnoActiveUser {
                    id: user.id,
                    name: user.name.clone(),
                    card_count: user.cards.len() as u32,
                })
                .collect(),
            host_user: self.host_user,
            user_turn: self.user_turn,
            is_ord: self.is_ord,
            last_card: match self.start_state {
                GameStartState::Setup | GameStartState::Ending => {
                    UnoCard::encode(false, UnoCardColour::Red, 0)
                }
                GameStartState::Active => self.last_card,
            },
            finished_users: self.finished_users.clone(),
            bust_users: self.bust_users.clone(),
        };

        for (user_id, sender) in self.user_senders.iter() {
            let user_cards = if self.start_state == GameStartState::Active {
                self.active_users
                    .iter()
                    .find(|u| u.id.eq(user_id))
                    .map(|u| u.cards.clone())
                    .unwrap_or_default()
            } else {
                vec![]
            };

            let msg = ServerUnoCommand::GameState(user_cards, state.clone());

            let Ok(encoded) = bincode::encode_to_vec(&msg, bincode::config::standard())
                .inspect_err(|err| println!("Failed to encode state for user {user_id} : {err:?}"))
            else {
                continue;
            };

            let _x = sender
                .send(ServerMessage::GameState(encoded))
                .inspect_err(|err| {
                    println!("Failed to send state to user {user_id} : {err:?}");
                });
        }
    }

    /// Steps when a user plays a card:
    ///
    /// 1. Check is player's turn
    /// 2. Check user has card
    /// 3. Apply card to game state
    /// 4. Update game users with new state
    fn submit_card(&mut self, user: u32, mut card: UnoCard) -> anyhow::Result<usize> {
        if !card.validate() {
            return Err(anyhow!("Invalid Card"));
        }

        let (is_power, mut colour, value) = card.decode();

        // When we receive a power card, black colour changers are stored as red
        //  but the colour submitted determines the colour to change to
        if is_power {
            match UnoCardPower::from(value) {
                UnoCardPower::PlusTwo | UnoCardPower::Skip | UnoCardPower::Reverse => {}
                UnoCardPower::ClrChange | UnoCardPower::PlusFour => {
                    println!("CHANGE COLOUR FOR BLACK CARD");
                    card = UnoCard::encode(is_power, UnoCardColour::Red, value);
                    colour = UnoCardColour::Red;
                }
            }
        }

        let (_, curr_colour, curr_value) = self.last_card.decode();

        // Black cards can be played on anything, just need to check regular cards
        if !card.is_black() && curr_colour != colour && curr_value != value {
            return Err(anyhow!("Card not allowed"));
        }

        let curr_user = &mut self.active_users[self.user_turn as usize];

        if curr_user.id != user {
            return Err(anyhow!("Not this user's turn"));
        }

        // I don't really like this implementation but I cba to think of
        //  anything better right now
        if let Some(idx) = curr_user
            .cards
            .iter()
            // We re encoded the card above to red for black cards
            //  as they are stored as red in the user's data
            .position(|&user_card| user_card == card)
        {
            curr_user.cards.remove(idx);
        } else {
            return Err(anyhow!("User does not have card"));
        }

        self.deck.discard(card);

        self.action
            .push(UnoAction::UserPlaceCard(curr_user.name.clone(), card));
        Ok(curr_user.cards.len())
    }

    fn commit_card(&mut self, card: UnoCard) {
        self.last_card = card;

        if card.is_power() {
            match UnoCardPower::from(card.get_value()) {
                UnoCardPower::PlusTwo => {
                    self.push_turn();
                    let curr_user = &mut self.active_users[self.user_turn as usize];

                    curr_user.cards.push(self.deck.pickup());
                    curr_user.cards.push(self.deck.pickup());

                    self.action
                        .push(UnoAction::UserPickup(curr_user.name.clone(), 2));

                    self.push_turn();
                }
                UnoCardPower::Skip => {
                    self.push_turn();
                    self.push_turn();
                }
                UnoCardPower::Reverse => {
                    self.is_ord = !self.is_ord;
                    self.push_turn();
                }
                UnoCardPower::PlusFour => {
                    self.push_turn();
                    let curr_user = &mut self.active_users[self.user_turn as usize];

                    curr_user.cards.push(self.deck.pickup());
                    curr_user.cards.push(self.deck.pickup());
                    curr_user.cards.push(self.deck.pickup());
                    curr_user.cards.push(self.deck.pickup());

                    self.action
                        .push(UnoAction::UserPickup(curr_user.name.clone(), 4));

                    self.push_turn();
                }
                UnoCardPower::ClrChange => {
                    self.push_turn();
                }
            }
        } else {
            self.push_turn();
        }
    }

    /// I may consider changing the type of user_turn
    ///  the amount of conversions is not ideal
    fn push_turn(&mut self) {
        if self.is_ord {
            if (self.user_turn as usize) == self.active_users.len() - 1 {
                self.user_turn = 0;
            } else {
                self.user_turn += 1
            }
        } else if self.user_turn == 0 {
            self.user_turn = (self.active_users.len() - 1) as u8;
        } else {
            self.user_turn -= 1;
        }
    }

    fn user_finished(&mut self, user_id: u32) {
        let user_idx = self
            .active_users
            .iter()
            .position(|user| user.id == user_id)
            .unwrap();

        let rm_user = self.active_users.remove(user_idx);

        self.finished_users.push((user_id, rm_user.name.clone()));
        self.action.push(UnoAction::UserFinished(rm_user.name));

        self.turn_from_leaver(user_idx);
    }

    fn check_user_bust(&mut self) {
        let mut leavers = vec![];
        let mut i: usize = 0;
        self.active_users.retain(|user| {
            i += 1;
            if user.cards.len() <= 20 {
                true
            } else {
                // Need to remove user here
                println!("USER BUST {}", user.name);
                self.action.push(UnoAction::UserBust(user.name.clone()));
                self.bust_users.push((user.id, user.name.clone()));
                leavers.push(i - 1);
                for card in user.cards.iter() {
                    self.deck.discard(*card);
                }
                false
            }
        });

        for leaver in leavers {
            self.turn_from_leaver(leaver);
        }
    }

    fn turn_from_leaver(&mut self, user_idx: usize) {
        let curr_idx = self.user_turn as usize;

        if curr_idx == self.active_users.len() {
            self.user_turn = curr_idx as u8 - 1;
        } else if curr_idx > user_idx {
            self.user_turn -= 1;
        }

        self.check_over();
    }

    fn check_over(&mut self) {
        if self.active_users.len() <= 1 {
            for user in self.active_users.drain(..) {
                self.finished_users.push((user.id, user.name));
            }
            self.action.push(UnoAction::GameEnded);
            self.start_state = GameStartState::Ending;
        }
    }
}

pub struct UnoDeck {
    main_deck: (u64, u64),
    discard_deck: (u64, u64),
}

impl UnoDeck {
    /// We are using the version of an uno deck with 108 cards ( no blanks )
    /// To do this, we fill the first u64 completely and assign 44 flags in the 2nd
    fn new() -> Self {
        UnoDeck {
            main_deck: (u64::MAX, (1u64 << 44) - 1),
            discard_deck: (0, 0),
        }
    }

    fn is_empty(&self) -> bool {
        self.main_deck.0 == 0 && self.main_deck.1 == 0
    }

    // We generate the pos outside of
    fn get_card(&mut self, mut pos: u8) -> UnoCard {
        assert!(
            !self.is_empty(),
            "Should be at least one card in deck when trying to pick up"
        );

        pos %= DECK_SIZE;

        // This is a terrible implementation
        // I'm going to switch it with a trailing zeros check instead
        // of this loop
        loop {
            if pos >= DECK_SIZE {
                pos -= DECK_SIZE;
            }

            let right_check = pos > 63;

            let check_cursor: u64 = 1 << (if right_check { pos - 64 } else { pos });

            let card_exist = if right_check {
                self.main_deck.1 & check_cursor != 0
            } else {
                self.main_deck.0 & check_cursor != 0
            };

            if !card_exist {
                pos += 1;
                continue;
            }

            // Desire
            // A   B   Q
            // 0   0   0
            // 0   1   0
            // 1   0   1
            // 1   1   0
            //
            //  A and not B
            if right_check {
                self.main_deck.1 &= !check_cursor;
            } else {
                self.main_deck.0 &= !check_cursor;
            };

            return Self::pos_to_card(pos);
        }
    }

    fn get_new_hand(&mut self, rng: &mut impl Rng) -> anyhow::Result<Vec<UnoCard>> {
        let mut hand = vec![];

        for _ in 0..10 {
            let pos = rng.random_range(0..DECK_SIZE);

            hand.push(self.get_card(pos));
        }

        Ok(hand)
    }

    fn pickup(&mut self) -> UnoCard {
        let pos = rand::random_range(0..DECK_SIZE) as u8;
        let card = self.get_card(pos);

        if self.is_empty() {
            self.main_deck = self.discard_deck;
            self.discard_deck = (0, 0);
        }

        card
    }

    fn discard(&mut self, card: UnoCard) {
        // Black cards can have 4 of them, possibly need to check all 4
        if card.is_black() {
            // All black cards are in the 2nd part of the deck, no need to scan
            let mut scan_flag = match UnoCardPower::from(card.and(0b00000111)) {
                UnoCardPower::PlusFour => 1u64 << 36,
                UnoCardPower::ClrChange => 1u64 << 40,
                _ => {
                    panic!("Colour validated as black but is not");
                }
            };

            for _ in 0..4 {
                if self.discard_deck.1 & scan_flag != 0 {
                    self.discard_deck.1 |= scan_flag;
                    return;
                }
                scan_flag <<= 1;
            }
            return;
        }

        let (pos, diff) = Self::card_to_pos(card);
        {
            let right_check = pos > 63;

            let check_cursor: u64 = 1u64 << (if right_check { pos - 64 } else { pos });

            if right_check {
                if self.discard_deck.1 & check_cursor == 0 {
                    self.discard_deck.1 |= check_cursor;
                    return;
                }
            } else if self.discard_deck.0 & check_cursor == 0 {
                self.discard_deck.0 |= check_cursor;
                return;
            };
        }

        if diff != 0 {
            let pos = pos + diff;
            let right_check = pos > 63;

            let check_cursor: u64 = 1u64 << (if right_check { pos - 64 } else { pos });

            if right_check {
                if self.discard_deck.1 & check_cursor == 0 {
                    self.discard_deck.1 |= check_cursor;
                    return;
                }
            } else if self.discard_deck.0 & check_cursor == 0 {
                self.discard_deck.0 |= check_cursor;
                return;
            };
        }

        println!("Tried to discard but no empty slot {:?}", card.decode());
    }

    /// This function may be a bit ass, should most likely switch it out
    ///  with a better match statement
    fn pos_to_card(pos: u8) -> UnoCard {
        // println!("Get from pos {pos} -> {}", pos % 4);
        assert!(pos < DECK_SIZE, "Malformed pos, too high");

        // Handling the case here for the 2 sets of base 1-9 cards
        if pos < 72 {
            let value = (pos / 8) + 1;
            let colour = UnoCardColour::from(pos % 4);

            return UnoCard::encode(false, colour, value);
        }

        // Handle the value 0 cards
        if pos < 76 {
            let colour = UnoCardColour::from(pos - 72);

            return UnoCard::encode(false, colour, 0);
        }

        // Coloured Power Cards
        if pos < 100 {
            let power = UnoCardPower::from((pos - 76) / 8);
            let colour = UnoCardColour::from((pos - 76) % 4);

            return UnoCard::encode(true, colour, power as u8);
        }

        let power = if pos < 104 {
            UnoCardPower::PlusFour
        } else {
            UnoCardPower::ClrChange
        };

        UnoCard::encode(true, UnoCardColour::Red, power as u8)
    }

    fn card_to_pos(card: UnoCard) -> (u8, u8) {
        assert!(
            !card.is_black(),
            "Only coloured cards can be checked here {card:?}"
        );

        let (is_power, colour, value) = card.decode();

        if value == 0 {
            return (76 + colour as u8, 0);
        }

        if !is_power {
            let pos = ((value - 1) * 8) + colour as u8;

            return (pos, colour as u8);
        }

        (76 + value + colour as u8, colour as u8)
    }
}

impl UnoUser {
    fn new_joiner(
        deck: &mut UnoDeck,
        user_id: u32,
        user: PlayerState,
    ) -> (UnoUser, UnboundedSender<ServerMessage>) {
        (
            UnoUser {
                id: user_id,
                name: user.name,
                cards: deck
                    .get_new_hand(&mut rand::rng())
                    .expect("Should be able to fmt deck here"),
            },
            user.sender.clone(),
        )
    }
}
