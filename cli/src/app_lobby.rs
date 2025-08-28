use crossterm::event::{Event, KeyCode, KeyEventKind};
use ratatui::{
    DefaultTerminal, Frame,
    layout::{Constraint, Direction, Layout},
    style::{Style, Stylize},
    symbols::border,
    text::{Line, Span, Text},
    widgets::{Block, Borders, Cell, Row, Table},
};
use rpc::{comms::ClientLobbyState, game_state::GameType};
use tokio::sync::mpsc::UnboundedReceiver;

use crate::AppMessage;

#[derive(Debug, Clone)]
pub struct AppLobby {
    pub name: String,
    pub id: u32,
    state: ClientLobbyState,
    view: LobbyView,
}

#[derive(Debug, Clone)]
enum LobbyView {
    Main(usize),
    Create(GameCreate),
}

#[derive(Debug, Clone)]
pub struct GameCreate {
    pub name: Vec<char>,
    pub game_type: GameType,
}

#[derive(Debug)]
pub enum LobbyResult {
    Exit,
    Create(GameCreate),
    Join(u32),
}

/// After a user authenticates, they get sent here.
/// We need to allow them to:
///
///  1. Create a new game
///  2. Join an existing game
///  3. Quit
///
impl AppLobby {
    pub fn new(name: String, id: u32, state: ClientLobbyState) -> AppLobby {
        AppLobby {
            name,
            id,
            state,
            view: LobbyView::Main(0),
        }
    }

    pub async fn start(
        mut self,
        terminal: &mut DefaultTerminal,
        app_receiver: &mut UnboundedReceiver<AppMessage>,
    ) -> anyhow::Result<LobbyResult> {
        terminal.draw(|frame| self.render(frame))?;
        while let Some(message) = app_receiver.recv().await {
            match message {
                AppMessage::RpcEvent(server_message) => match server_message {
                    rpc::comms::ServerMessage::AuthResponse(_) => {
                        todo!("Really should never get this response again???")
                    }
                    rpc::comms::ServerMessage::LobbyState(new_state) => self.state = new_state,
                    rpc::comms::ServerMessage::NewPlayerCount(player_count) => {
                        self.state.player_count = player_count
                    }
                    rpc::comms::ServerMessage::GameState(_) => {}
                    rpc::comms::ServerMessage::JoinedGame(_, _) => {}
                },
                AppMessage::TerminalEvent(event) => {
                    if let Event::Key(key_event) = event
                        && key_event.kind == KeyEventKind::Release
                    {
                        match key_event.code {
                            // KeyCode::Left => !(),
                            // KeyCode::Right => !(),
                            KeyCode::Up => match self.view {
                                LobbyView::Main(idx) => {
                                    if self.state.games.is_empty() {
                                        continue;
                                    }
                                    let new_idx = if idx == 0 {
                                        self.state.games.len() - 1
                                    } else {
                                        idx - 1
                                    };
                                    self.view = LobbyView::Main(new_idx);
                                }
                                LobbyView::Create(_) => {}
                            },
                            KeyCode::Down => match self.view {
                                LobbyView::Main(idx) => {
                                    if self.state.games.is_empty() {
                                        continue;
                                    }

                                    let new_idx = if idx >= self.state.games.len() - 1 {
                                        0
                                    } else {
                                        idx + 1
                                    };
                                    self.view = LobbyView::Main(new_idx);
                                }
                                LobbyView::Create(_) => {}
                            },
                            KeyCode::Char(c) => match &mut self.view {
                                LobbyView::Main(_) => match c {
                                    'c' => {
                                        self.view = LobbyView::Create(GameCreate {
                                            name: vec![],
                                            game_type: GameType::Uno,
                                        })
                                    }
                                    _ => {}
                                },
                                LobbyView::Create(create) => {
                                    create.name.push(c);
                                }
                            },
                            KeyCode::Backspace => match &mut self.view {
                                LobbyView::Main(_) => {}
                                LobbyView::Create(create) => {
                                    create.name.pop();
                                }
                            },
                            KeyCode::Enter => match self.view {
                                LobbyView::Main(idx) => {
                                    if let Some(game) = self.state.games.get(idx) {
                                        return Ok(LobbyResult::Join(game.id));
                                    }
                                }
                                LobbyView::Create(game_create) => {
                                    return Ok(LobbyResult::Create(game_create.clone()));
                                }
                            },

                            // KeyCode::Tab => !(),
                            KeyCode::Esc => match self.view {
                                LobbyView::Main(_) => break,
                                LobbyView::Create(_) => self.view = LobbyView::Main(0),
                            },

                            // KeyCode::Home => !(),
                            // KeyCode::End => !(),
                            // KeyCode::PageUp => !(),
                            // KeyCode::PageDown => !(),
                            // KeyCode::BackTab => !(),
                            // KeyCode::Delete => !(),
                            // KeyCode::Insert => !(),
                            // KeyCode::F(_) => !(),
                            // KeyCode::Null => !(),
                            // KeyCode::CapsLock => !(),
                            // KeyCode::ScrollLock => !(),
                            // KeyCode::NumLock => !(),
                            // KeyCode::PrintScreen => !(),
                            // KeyCode::Pause => !(),
                            // KeyCode::Menu => !(),
                            // KeyCode::KeypadBegin => !(),
                            // KeyCode::Media(media_key_code) => !(),
                            // KeyCode::Modifier(modifier_key_code) => !(),
                            _ => {}
                        }
                    }
                    // match event {
                    //     crossterm::event::Event::FocusGained => !(),
                    //     crossterm::event::Event::FocusLost => !(),
                    //     crossterm::event::Event::Key(key_event) => !(),
                    //     crossterm::event::Event::Mouse(mouse_event) => ,
                    //     crossterm::event::Event::Paste(_) => !(),
                    //     crossterm::event::Event::Resize(_, _) => !(),
                    // }
                }
                AppMessage::Failure(err) => {
                    println!("Received Some Failure {err:?}");
                    return Err(err);
                }
            }
            terminal.draw(|frame| self.render(frame))?;
        }

        Ok(LobbyResult::Exit)
    }

    /// Need to display:
    ///
    /// Main screen:
    ///  Main commands to create / join a game
    ///  Active Games
    ///
    /// Create game:
    ///  Name
    ///  Type Selector
    fn render(&self, frame: &mut Frame) {
        match &self.view {
            LobbyView::Main(idx) => self.main_view(frame, *idx),
            LobbyView::Create(create) => self.create_view(frame, create),
        }
    }

    fn main_view(&self, frame: &mut Frame, idx: usize) {
        let area = frame.area();

        // Outer container with borders
        let outer = self.get_block();
        frame.render_widget(outer.clone(), area);

        // Get the inner area (inside the borders)
        let inner = outer.inner(area);

        // Split inner area into left and right halves
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(44), Constraint::Fill(1)])
            .split(inner);

        let mut main_text = Text::from("Welcome to Tempest!");
        main_text.push_line(Line::from(""));
        main_text.push_line(Line::from("Press \"c\" to create a new game"));
        main_text.push_line(Line::from("Press Up / Down to chose a game to join"));
        main_text.push_line(Line::from("Press Enter to join a selected game"));

        frame.render_widget(main_text, chunks[0]);
        frame.render_widget(self.game_list(idx), chunks[1]);

        // println!("Render here");
        frame.render_widget(self.get_block(), frame.area())
    }

    // Handle Index stuff later
    fn game_list(&self, idx: usize) -> Table<'_> {
        let rows: Vec<Row<'_>> = self
            .state
            .games
            .iter()
            .enumerate()
            .map(|(i, game)| {
                let (game_cell, user_cell) = match game.game_type {
                    GameType::Uno => (
                        Cell::new("Uno").light_cyan(),
                        Cell::new(format!("{} / 4", game.active_players)).gray(),
                    ),
                };

                Row::new(vec![
                    Cell::new(if i == idx { ">" } else { " " }).blue(),
                    Cell::new(game.name.clone()),
                    game_cell,
                    user_cell,
                ])
            })
            .collect();

        let widths = vec![
            Constraint::Length(1),
            Constraint::Length(15),
            Constraint::Length(4),
            Constraint::Fill(1),
        ];

        let list_border = border::Set {
            vertical_left: "┆",
            ..Default::default()
        };

        let block = Block::new()
            .title_top(Line::from("Select a Game to join").bold().white())
            .borders(Borders::LEFT)
            .border_set(list_border)
            .border_style(Style::new().gray());

        Table::new(rows, widths)
            .style(Style::default().white())
            .block(block)
    }

    fn create_view(&self, frame: &mut Frame, create: &GameCreate) {
        let area = frame.area();

        // Outer container with borders
        let outer = self.get_block();
        frame.render_widget(outer.clone(), area);

        // Get the inner area (inside the borders)
        let inner = outer.inner(area);

        // Split inner area into left and right halves
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Fill(1), Constraint::Length(1)])
            .split(inner);

        let mut main_text = Text::from("Create a new Game! ( Currently Only Uno Available )");
        main_text.push_line(Line::from(""));
        main_text.push_line(Line::from("Lobby Name: "));
        main_text.push_line(Line::from(vec![
            Span::from("> ").blue(),
            Span::from(String::from_iter(&create.name)),
        ]));

        frame.render_widget(main_text, chunks[0]);
        frame.render_widget(self.get_block(), frame.area())
    }

    fn get_block(&self) -> Block<'_> {
        Block::bordered()
            .border_style(Style::new().light_blue())
            .title_top(
                Line::from(format!(" Tempest ~ {} ~ Lobby", self.name))
                    .bold()
                    .white(),
            )
            .title_bottom(Line::from(" Esc to quit ").bold().white().right_aligned())
            .title_bottom(
                Line::from(format!(" Players Online: {} ", self.state.player_count)).white(),
            )
    }
}
