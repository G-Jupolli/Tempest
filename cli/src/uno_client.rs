use anyhow::anyhow;
use bincode::config::Configuration;
use crossterm::event::{Event, KeyCode, KeyEventKind};
use ratatui::{
    DefaultTerminal, Frame,
    layout::{Constraint, Direction, Layout, Margin, Rect},
    style::{Color, Style, Stylize},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Cell, Clear, Padding, Paragraph, Row, Table},
};
use rpc::{
    comms::{ClientAuthedCommand, ClientGameCommand, ClientMessage, ServerMessage, TcpSender},
    game_state::{self, GameStartState},
    uno::{
        ServerUnoCommand, UnoAction, UnoCard, UnoCardColour, UnoCardPower, UnoClientAction,
        UnoClientGameState,
    },
};
use std::cmp::min;
use tokio::sync::mpsc;

use crate::AppMessage;

struct PlayCard {
    card: UnoCard,
    clr_idx: usize,
}

const HELP_TEXT: &str = r#"How to play:
When it's your turn, use the arrow keys to select a card.
Press enter to select, and then also to confirm your card.
To play a card it will need to match either the colour
 or the value of the last played card.
Blank "P" cards can be place on top of anything.
Using a "P" card requires picking a colour to change the stack to.

Some Cards can have actions which are:
 Sk ~ Skip next user's turn
 Rv ~ Reverse the turn order
 +2 ~ Next suer takes 2 cards and skips turn
 +4 ~ Next user takes 4 cards and skips turn
 Cc ~ Change the colour of the stack
"#;

pub struct UnoClient;

impl UnoClient {
    pub async fn try_start(
        lobby: String,
        user_name: String,
        user_id: u32,
        tcp_sender: &mut TcpSender<ClientMessage>,
        app_receiver: &mut mpsc::UnboundedReceiver<AppMessage>,
        terminal: &mut DefaultTerminal,
    ) -> anyhow::Result<()> {
        terminal.draw(|frame| Self::render_loading(frame, user_name.as_str(), lobby.as_str()))?;

        let mut server_state = None;

        while let Some(msg) = app_receiver.recv().await {
            match msg {
                AppMessage::RpcEvent(server_message) => match server_message {
                    ServerMessage::GameState(data) => {
                        match Self::decode_uno_server_command(data)?.0 {
                            ServerUnoCommand::GameState(cards, uno_client_game_state) => {
                                server_state = Some((cards, uno_client_game_state));
                                break;
                            }
                        }
                    }
                    _ => {
                        println!("Needless server command");
                    }
                },
                AppMessage::TerminalEvent(_) => {
                    println!("Impl exit to leave here");
                }
                AppMessage::Failure(err) => {
                    return Err(err);
                }
            }
        }

        let Some(server_state) = server_state else {
            return Err(anyhow!("Didn't get game state"));
        };

        let res = Self::start(
            lobby,
            user_id,
            server_state.1,
            server_state.0,
            tcp_sender,
            app_receiver,
            terminal,
        )
        .await;

        tcp_sender
            .send(&ClientMessage::Authed(
                user_id,
                ClientAuthedCommand::Game(ClientGameCommand::Leave),
            ))
            .await?;

        res
    }

    async fn start(
        lobby: String,
        user_id: u32,
        mut server_state: UnoClientGameState,
        mut my_cards: Vec<UnoCard>,
        tcp_sender: &mut TcpSender<ClientMessage>,
        app_receiver: &mut mpsc::UnboundedReceiver<AppMessage>,
        terminal: &mut DefaultTerminal,
    ) -> anyhow::Result<()> {
        let mut events: Vec<UnoAction> = server_state.action.drain(..).collect();
        let mut card_idx: usize = 0;
        let mut card_to_play: Option<PlayCard> = None;

        terminal.draw(|frame| {
            Self::render(
                frame,
                user_id,
                &lobby,
                &server_state,
                &my_cards,
                &events,
                card_idx,
            );
            if let Some(play_card) = &card_to_play {
                Self::render_play_card(frame, play_card);
            }
        })?;

        while let Some(msg) = app_receiver.recv().await {
            match msg {
                AppMessage::RpcEvent(server_message) => match server_message {
                    ServerMessage::GameState(items) => {
                        match Self::decode_uno_server_command(items)?.0 {
                            ServerUnoCommand::GameState(uno_cards, mut uno_client_game_state) => {
                                my_cards = uno_cards;
                                let mut server_actions =
                                    uno_client_game_state.action.drain(..).collect();
                                events.append(&mut server_actions);

                                server_state = uno_client_game_state;
                            }
                        }
                    }
                    _ => {}
                },
                AppMessage::TerminalEvent(event) => match event {
                    Event::Key(key_event) => {
                        if key_event.kind != KeyEventKind::Release {
                            continue;
                        }
                        match key_event.code {
                            KeyCode::Enter => {
                                match server_state.game_state {
                                    game_state::GameStartState::Setup => {
                                        if server_state.host_user == user_id {
                                            let _x = tcp_sender
                                                .send(&ClientMessage::Authed(
                                                    user_id,
                                                    ClientAuthedCommand::Game(
                                                        ClientGameCommand::Start,
                                                    ),
                                                ))
                                                .await;
                                        }
                                    }
                                    game_state::GameStartState::Ending => {
                                        continue;
                                    }
                                    game_state::GameStartState::Active => {}
                                };

                                let is_turn = server_state
                                    .active_users
                                    .get(server_state.user_turn as usize)
                                    .is_some_and(|usr| usr.id == user_id);

                                if !is_turn {
                                    continue;
                                }

                                match card_to_play {
                                    Some(mut card) => {
                                        if card.card.is_black() {
                                            card.card = UnoCard::encode(
                                                true,
                                                UnoCardColour::from(card.clr_idx as u8),
                                                card.card.get_value(),
                                            );
                                        }

                                        tcp_sender
                                            .send(&Self::encode_uno_client_command(
                                                user_id,
                                                UnoClientAction::PlayCard(card.card),
                                            )?)
                                            .await?;

                                        card_to_play = None;
                                    }
                                    None => {
                                        card_to_play = my_cards
                                            .get(card_idx)
                                            .copied()
                                            .map(|card| PlayCard { card, clr_idx: 0 });
                                    }
                                }
                            }
                            KeyCode::Left => {
                                if let Some(playing) = &mut card_to_play {
                                    if playing.clr_idx == 0 {
                                        playing.clr_idx = 3;
                                    } else {
                                        playing.clr_idx -= 1;
                                    }
                                } else {
                                    if my_cards.is_empty() {
                                        continue;
                                    }
                                    if card_idx == 0 {
                                        card_idx = my_cards.len() - 1;
                                    } else {
                                        card_idx -= 1;
                                    }
                                }
                            }
                            KeyCode::Right => {
                                if let Some(playing) = &mut card_to_play {
                                    if playing.clr_idx == 3 {
                                        playing.clr_idx = 0;
                                    } else {
                                        playing.clr_idx += 1;
                                    }
                                } else {
                                    if my_cards.is_empty() {
                                        continue;
                                    }

                                    if card_idx >= my_cards.len() - 1 {
                                        card_idx = 0;
                                    } else {
                                        card_idx += 1;
                                    }
                                }
                            }
                            KeyCode::Up | KeyCode::Down => {
                                if card_to_play.is_some() || my_cards.len() <= 10 {
                                    continue;
                                }

                                if card_idx > 10 {
                                    card_idx -= 10
                                } else {
                                    card_idx = min(my_cards.len() - 1, card_idx + 10)
                                }
                            }
                            KeyCode::Char(c) => {
                                if c == 'p' {
                                    let is_turn = server_state
                                        .active_users
                                        .get(server_state.user_turn as usize)
                                        .is_some_and(|usr| usr.id == user_id);

                                    if !is_turn {
                                        continue;
                                    }

                                    tcp_sender
                                        .send(&Self::encode_uno_client_command(
                                            user_id,
                                            UnoClientAction::PickupCard,
                                        )?)
                                        .await?;
                                }
                            }
                            KeyCode::Esc => {
                                if card_to_play.is_some() {
                                    card_to_play = None;
                                } else {
                                    return Ok(());
                                }
                            }
                            _ => {
                                continue;
                            }
                        }
                    }
                    Event::Resize(_, _) => {}
                    _ => {
                        continue;
                    }
                },
                AppMessage::Failure(err) => {
                    return Err(err);
                }
            }

            terminal.draw(|frame| {
                Self::render(
                    frame,
                    user_id,
                    &lobby,
                    &server_state,
                    &my_cards,
                    &events,
                    card_idx,
                );
                if let Some(play_card) = &card_to_play {
                    Self::render_play_card(frame, play_card);
                }
            })?;
        }

        Err(anyhow!("Internal Failure 6712354"))
    }

    /// Idea for playing field:
    ///
    /// +------+------+------~
    /// | User | Last | Events
    /// | List | Card |
    /// +------+------+------~
    /// |             |
    /// | Local Cards | Help
    /// |             |
    /// +-------------+------~
    fn render(
        frame: &mut Frame,
        user_id: u32,
        lobby: &str,
        server_state: &UnoClientGameState,
        my_cards: &[UnoCard],
        events: &[UnoAction],
        card_idx: usize,
    ) {
        let turn_name = match server_state.game_state {
            game_state::GameStartState::Setup => " Waiting To Start ".to_string(),
            game_state::GameStartState::Active => {
                let active_user = server_state
                    .active_users
                    .get(server_state.user_turn as usize)
                    .expect("Should have user here");
                if active_user.id == user_id {
                    " My Turn ".to_string()
                } else {
                    format!(" {}'s Turn ", active_user.name)
                }
            }
            game_state::GameStartState::Ending => " Finished ".to_string(),
        };

        // Making the Main Container here, and everything will sit inside of it
        let outer_block = Block::bordered()
            .border_style(Style::new().light_blue())
            .title_top(
                Line::from(format!(" Tempest ~ {lobby} ( Uno ) "))
                    .bold()
                    .white(),
            )
            .title(Line::from(turn_name).bold().white().centered())
            .title_bottom(Line::from(" Esc to quit ").bold().white().right_aligned());

        let area = frame.area();
        frame.render_widget(outer_block.clone(), area);

        // This will put everything inside of the area with padding 1
        // It kind of messes with the borders but I'll have to figure
        //  some other display if I don't find a fix
        let inner = outer_block.inner(area);

        let row_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(6), Constraint::Fill(1)])
            .split(inner);

        let top_columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(35),
                Constraint::Length(25),
                Constraint::Fill(1),
            ])
            .split(row_chunks[0]);

        // User List
        frame.render_widget(
            Self::user_list(server_state).block(Block::default().borders(Borders::RIGHT)),
            top_columns[0],
        );

        match server_state.game_state {
            GameStartState::Setup => {
                frame.render_widget(
                    Paragraph::new("Waiting for host\nto start")
                        .centered()
                        .block(Block::default().borders(Borders::RIGHT)),
                    top_columns[1],
                );
            }
            GameStartState::Active => {
                // Last Played
                frame.render_widget(
                    Paragraph::new("Last Card").block(Block::default().borders(Borders::RIGHT)),
                    top_columns[1],
                );

                let mut card_inner = top_columns[1].inner(Margin::new(8, 1));

                card_inner.width = 5;
                card_inner.height = 4;

                frame.render_widget(Self::card_text(&server_state.last_card, false), card_inner);
            }
            GameStartState::Ending => {
                frame.render_widget(
                    Paragraph::new("Game over\nThank you for playing!")
                        .centered()
                        .block(Block::default().borders(Borders::RIGHT)),
                    top_columns[1],
                );
            }
        }

        // Events
        Self::event_list(frame, top_columns[2], events);

        // --- Bottom row: 2 columns --
        let bottom_columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(60), Constraint::Fill(40)])
            .split(row_chunks[1]);

        // Local Cards
        match server_state.game_state {
            GameStartState::Setup => {
                if server_state.host_user == user_id {
                    frame.render_widget(
                Paragraph::new("You can start the game by pressing Enter\nif there are at least two people in the lobby")
                    .centered()
                    .block(Block::default().borders(Borders::RIGHT)),
                bottom_columns[0],
            );
                } else {
                    frame.render_widget(
                        Paragraph::new("Waiting for the host to start the game")
                            .centered()
                            .block(Block::default().borders(Borders::RIGHT)),
                        bottom_columns[0],
                    );
                }
            }
            GameStartState::Active => {
                frame.render_widget(
                    Paragraph::new("My Cards").block(Block::default().borders(Borders::RIGHT)),
                    bottom_columns[0],
                );

                Self::my_cards(frame, bottom_columns[0], my_cards, card_idx);
            }
            GameStartState::Ending => {}
        }

        // Help
        frame.render_widget(
            Paragraph::new(HELP_TEXT).block(Block::default()),
            bottom_columns[1],
        );
    }

    fn render_play_card(frame: &mut Frame, card_data: &PlayCard) {
        let mut inner = frame.area().inner(Margin {
            horizontal: 20,
            vertical: 5,
        });

        inner.width = 25;
        inner.height = 5;

        frame.render_widget(
            Clear,
            Rect {
                x: inner.x - 1,
                y: inner.y - 1,
                width: 27,
                height: 7,
            },
        );

        let text = if card_data.card.is_black() {
            let select = format!("{:width$}v", " ", width = (card_data.clr_idx * 4) + 3);

            let clr_line = vec![
                Span::from("   R").light_red(),
                Span::from("   B").light_blue(),
                Span::from("   G").light_green(),
                Span::from("   Y").light_yellow(),
            ];

            Text::from(vec![
                Line::from("Color to play Card as"),
                Line::from(select).bold().white(),
                Line::from(clr_line),
            ])
        } else {
            Text::from(vec![
                Line::from(""),
                Line::from(vec![
                    Span::from("    "),
                    Self::card_for_event(&card_data.card),
                ]),
                Line::from(""),
            ])
        };

        frame.render_widget(
            Paragraph::new(text).block(
                Block::bordered()
                    .border_style(Style::new().light_blue())
                    .title_top(Line::from(" Play Card? ").bold().white())
                    .title_bottom(Line::from(" Esc to return ").bold().white().right_aligned())
                    .bg(Color::Reset)
                    .padding(Padding::horizontal(1)),
            ),
            inner,
        )
    }

    fn event_list(frame: &mut Frame, area: Rect, events: &[UnoAction]) {
        let skip = if events.len() > 5 {
            events.len() - 5
        } else {
            0
        };

        let lines: Vec<Line> = events
            .iter()
            .skip(skip)
            .map(|ev| match ev {
                UnoAction::Init => Line::from("Server Started"),
                UnoAction::InitialCard(uno_card) => Line::from(vec![
                    Span::from("Initial Card: "),
                    Self::card_for_event(uno_card),
                ]),
                UnoAction::UserPlaceCard(user, uno_card) => Line::from(vec![
                    Span::from(format!("{user} placed: ")),
                    Self::card_for_event(uno_card),
                ]),
                UnoAction::UserPickup(user, count) => {
                    Line::from(format!("{user} picked up {count} card(s) "))
                }
                UnoAction::UserJoined(user) => Line::from(format!("{user} Joined ")),
                UnoAction::UserLeft(user) => Line::from(format!("{user} Left ")),
                UnoAction::UserFinished(user) => Line::from(format!("{user} Finished ")),
                UnoAction::UserBust(user) => Line::from(format!("{user} Bust ")),
                UnoAction::GameEnded => Line::from("Game Over"),
            })
            .collect();

        frame.render_widget(
            Paragraph::new(Text::from(lines)).block(Block::default()),
            area,
        );
    }

    fn card_for_event(card: &UnoCard) -> Span {
        let (power, colour, value) = card.decode();

        let clr_char = match colour {
            rpc::uno::UnoCardColour::Red => "Red",
            rpc::uno::UnoCardColour::Blue => "Blue",
            rpc::uno::UnoCardColour::Green => "Green",
            rpc::uno::UnoCardColour::Yellow => "Yellow",
        };

        let value_str = if power {
            match UnoCardPower::from(value) {
                UnoCardPower::PlusTwo => "+2",
                UnoCardPower::Skip => "Skip",
                UnoCardPower::Reverse => "Reverse",
                UnoCardPower::PlusFour => "+4",
                UnoCardPower::ClrChange => "Change Colour",
            }
            .to_string()
        } else {
            value.to_string()
        };

        let span = Span::from(format!("{clr_char} ~ {value_str}"));

        match colour {
            UnoCardColour::Red => span.light_red(),
            UnoCardColour::Blue => span.light_blue(),
            UnoCardColour::Green => span.light_green(),
            UnoCardColour::Yellow => span.light_yellow(),
        }
    }

    fn user_list(server_state: &UnoClientGameState) -> Table {
        let idx = server_state.user_turn as usize;

        let mut rows: Vec<Row<'_>> = server_state
            .finished_users
            .iter()
            .enumerate()
            .map(|(i, (_, name))| {
                Row::new(vec![
                    Cell::new((i + 1).to_string()).light_green(),
                    Cell::new(name.to_string()).light_green(),
                ])
            })
            .collect();

        let ord_str = if server_state.is_ord { "v " } else { "^ " };

        for (_, user) in server_state.bust_users.iter() {
            let new_row = Row::new(vec![
                Cell::new("x").light_red(),
                Cell::new(user.to_string()).light_red(),
            ]);

            rows.push(new_row);
        }

        for (i, user) in server_state.active_users.iter().enumerate() {
            let new_row = Row::new(vec![
                Cell::new(if i == idx { ord_str } else { " " }).light_green(),
                Cell::new(user.name.clone()),
                Cell::new(user.card_count.to_string()),
            ]);

            rows.push(new_row);
        }

        let widths = vec![
            Constraint::Length(1),
            Constraint::Length(15),
            Constraint::Length(4),
            Constraint::Fill(1),
        ];

        Table::new(rows, widths).style(Style::default().white())
    }

    fn my_cards(frame: &mut Frame, mut area: Rect, cards: &[UnoCard], card_idx: usize) {
        for (i, card) in cards.iter().enumerate() {
            let rect = Rect {
                x: area.x + ((i % 10) * 6) as u16,
                y: area.y + ((i / 10) * 5) as u16 + 2,
                width: 5,
                height: 4,
            };
            let para = Self::card_text(card, true);

            frame.render_widget(para, rect);
        }

        area.x += ((card_idx % 10) * 6) as u16 + 2;
        area.y += ((card_idx / 10) * 5) as u16 + 1;
        area.height = 1;
        area.width = 1;

        frame.render_widget(Paragraph::new("v").bold().white(), area);
    }

    fn card_text(card: &UnoCard, allow_black: bool) -> Paragraph {
        let (power, colour, value) = card.decode();

        let clr_char = if allow_black && card.is_black() {
            " P"
        } else {
            match colour {
                rpc::uno::UnoCardColour::Red => " R",
                rpc::uno::UnoCardColour::Blue => " B",
                rpc::uno::UnoCardColour::Green => " G",
                rpc::uno::UnoCardColour::Yellow => " Y",
            }
        };

        let value_str = if power {
            match UnoCardPower::from(value) {
                UnoCardPower::PlusTwo => "+2",
                UnoCardPower::Skip => "Sk",
                UnoCardPower::Reverse => "Rv",
                UnoCardPower::PlusFour => "+4",
                UnoCardPower::ClrChange => "Cc",
            }
            .to_string()
        } else {
            format!(" {value}")
        };

        let mut inner_block = Block::bordered();

        if allow_black && card.is_black() {
            inner_block = inner_block.white()
        } else {
            match colour {
                UnoCardColour::Red => inner_block = inner_block.light_red(),
                UnoCardColour::Blue => inner_block = inner_block.light_blue(),
                UnoCardColour::Green => inner_block = inner_block.light_green(),
                UnoCardColour::Yellow => inner_block = inner_block.light_yellow(),
            }
        }

        Paragraph::new(Text::from(format!("{clr_char}\n{value_str}"))).block(inner_block)
    }

    fn render_loading(frame: &mut Frame, user_name: &str, lobby: &str) {
        frame.render_widget(
            Paragraph::new(Text::from(Line::from("Loading Into Server").bold())).block(
                Block::bordered()
                    .border_style(Style::new().light_blue())
                    .title_top(
                        Line::from(format!(" Tempest ~ {user_name} ~ {lobby} "))
                            .bold()
                            .white(),
                    )
                    .title_bottom(Line::from(" Esc to quit ").bold().white().right_aligned()),
            ),
            frame.area(),
        )
    }

    fn decode_uno_server_command(data: Vec<u8>) -> anyhow::Result<(ServerUnoCommand, usize)> {
        bincode::decode_from_slice::<ServerUnoCommand, Configuration>(
            &data,
            bincode::config::standard(),
        )
        .map_err(|err| anyhow!("Failed decode").context(err))
    }

    fn encode_uno_client_command(
        user_id: u32,
        command: UnoClientAction,
    ) -> anyhow::Result<ClientMessage> {
        let raw_enc = bincode::encode_to_vec(command, bincode::config::standard())?;

        Ok(ClientMessage::Authed(
            user_id,
            ClientAuthedCommand::Game(ClientGameCommand::Raw(raw_enc)),
        ))
    }
}
