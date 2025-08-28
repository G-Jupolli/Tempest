use crossterm::event::{Event, KeyEventKind};
use ratatui::{
    DefaultTerminal, Frame,
    style::{Style, Stylize},
    text::{Line, Text},
    widgets::{Block, Paragraph},
};
use rpc::comms::{ClientLobbyState, ClientMessage, TcpSender};
use tokio::sync::mpsc::UnboundedReceiver;

use crate::AppMessage;

pub struct GameClient {
    name: String,
    id: u32,
    state: GameState,
    sender: TcpSender<ClientMessage>,
}

enum GameState {
    Lobby(ClientLobbyState),
    Game,
}

impl GameClient {
    pub fn new(
        name: String,
        id: u32,
        lobby_state: ClientLobbyState,
        sender: TcpSender<ClientMessage>,
    ) -> GameClient {
        GameClient {
            name,
            id,
            state: GameState::Lobby(lobby_state),
            sender,
        }
    }

    pub async fn start(
        mut self,
        mut terminal: DefaultTerminal,
        mut app_receiver: UnboundedReceiver<AppMessage>,
    ) -> anyhow::Result<()> {
        while let Some(message) = app_receiver.recv().await {
            match message {
                AppMessage::RpcEvent(server_message) => {
                    println!("Received Server Message {server_message:?}");
                }
                AppMessage::TerminalEvent(event) => {
                    if let Event::Key(key_event) = event
                        && key_event.kind == KeyEventKind::Release
                    {
                        println!("Key released {key_event:?}");

                        match key_event.code {
                            crossterm::event::KeyCode::Backspace => todo!(),
                            crossterm::event::KeyCode::Enter => todo!(),
                            crossterm::event::KeyCode::Left => todo!(),
                            crossterm::event::KeyCode::Right => todo!(),
                            crossterm::event::KeyCode::Up => todo!(),
                            crossterm::event::KeyCode::Down => todo!(),
                            crossterm::event::KeyCode::Home => todo!(),
                            crossterm::event::KeyCode::End => todo!(),
                            crossterm::event::KeyCode::PageUp => todo!(),
                            crossterm::event::KeyCode::PageDown => todo!(),
                            crossterm::event::KeyCode::Tab => todo!(),
                            crossterm::event::KeyCode::BackTab => todo!(),
                            crossterm::event::KeyCode::Delete => todo!(),
                            crossterm::event::KeyCode::Insert => todo!(),
                            crossterm::event::KeyCode::F(_) => todo!(),
                            crossterm::event::KeyCode::Char(_) => todo!(),
                            crossterm::event::KeyCode::Null => todo!(),
                            crossterm::event::KeyCode::Esc => todo!(),
                            crossterm::event::KeyCode::CapsLock => todo!(),
                            crossterm::event::KeyCode::ScrollLock => todo!(),
                            crossterm::event::KeyCode::NumLock => todo!(),
                            crossterm::event::KeyCode::PrintScreen => todo!(),
                            crossterm::event::KeyCode::Pause => todo!(),
                            crossterm::event::KeyCode::Menu => todo!(),
                            crossterm::event::KeyCode::KeypadBegin => todo!(),
                            crossterm::event::KeyCode::Media(media_key_code) => todo!(),
                            crossterm::event::KeyCode::Modifier(modifier_key_code) => todo!(),
                        }
                    }
                    // match event {
                    //     crossterm::event::Event::FocusGained => todo!(),
                    //     crossterm::event::Event::FocusLost => todo!(),
                    //     crossterm::event::Event::Key(key_event) => todo!(),
                    //     crossterm::event::Event::Mouse(mouse_event) => ,
                    //     crossterm::event::Event::Paste(_) => todo!(),
                    //     crossterm::event::Event::Resize(_, _) => todo!(),
                    // }
                }
                AppMessage::Failure(error) => {
                    println!("Received Some Failure {error:?}");
                }
            }

            terminal.draw(|frame| self.render(frame))?;
        }

        Ok(())
    }

    fn render(&self, frame: &mut Frame) {
        frame.render_widget(
            Paragraph::new(Text::from(Line::from("In game client").bold())).block(self.get_block()),
            frame.area(),
        )
    }

    fn get_block(&self) -> Block<'_> {
        Block::bordered()
            .border_style(Style::new().light_blue())
            .title_top(
                Line::from(format!(
                    " Tempest ~ {} ~ {}",
                    self.name,
                    self.client_status()
                ))
                .bold()
                .white(),
            )
            .title_bottom(Line::from(" Esc to quit ").bold().white().right_aligned())
    }

    fn client_status(&self) -> String {
        match self.state {
            GameState::Lobby(_) => return "Lobby".to_string(),
            GameState::Game => todo!(),
        }
    }
}
