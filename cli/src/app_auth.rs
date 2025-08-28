use anyhow::anyhow;
use crossterm::event::{Event, KeyEventKind};
use ratatui::{
    DefaultTerminal, Frame,
    style::{Style, Stylize},
    text::{Line, Span, Text},
    widgets::{Block, Paragraph},
};
use rpc::comms::{
    ClientLobbyState, ClientMessage, ServerMessage, TcpReceiver, TcpSender, split_stream,
};
use tokio::{net::TcpStream, sync::mpsc::UnboundedReceiver};

use crate::{AppMessage, app_lobby::AppLobby};

pub struct AppAuth;

impl AppAuth {
    pub async fn start_auth_loop(
        terminal: &mut DefaultTerminal,
        receiver: &mut UnboundedReceiver<AppMessage>,
    ) -> anyhow::Result<(
        AppLobby,
        TcpSender<ClientMessage>,
        TcpReceiver<ServerMessage>,
    )> {
        let mut name: Vec<char> = vec![];
        let mut server: Vec<char> = vec![];
        let mut is_name: bool = true;

        terminal.draw(|frame| AppAuth::render(frame, is_name, &name, &server))?;

        while let Some(msg) = receiver.recv().await {
            let terminal_event = match msg {
                AppMessage::RpcEvent(_) => {
                    panic!("RPC should not be active at this point")
                }
                AppMessage::Failure(error) => {
                    panic!("Should not have failure at this point, just end service, {error:?}")
                }
                AppMessage::TerminalEvent(event) => event,
            };

            let Event::Key(key_event) = terminal_event else {
                continue;
            };

            if key_event.kind != KeyEventKind::Release {
                continue;
            }

            match key_event.code {
                crossterm::event::KeyCode::Backspace => {
                    if is_name {
                        name.pop();
                    } else {
                        server.pop();
                    }
                }
                crossterm::event::KeyCode::Enter => {
                    // This gets hit when the cli is initialized too
                    if name.is_empty() {
                        continue;
                    }

                    terminal.draw(Self::render_loading)?;

                    return Self::try_connect(String::from_iter(name), String::from_iter(server))
                        .await;
                }
                crossterm::event::KeyCode::Up
                | crossterm::event::KeyCode::Down
                | crossterm::event::KeyCode::Tab => {
                    is_name = !is_name;
                }
                crossterm::event::KeyCode::Char(c) => {
                    if is_name {
                        name.push(c);
                    } else {
                        server.push(c);
                    }
                }
                crossterm::event::KeyCode::Esc => {
                    return Err(anyhow!("is Exit"));
                }
                // crossterm::event::KeyCode::Left => todo!(),
                // crossterm::event::KeyCode::Right => todo!(),
                // crossterm::event::KeyCode::Home => todo!(),
                // crossterm::event::KeyCode::End => todo!(),
                // crossterm::event::KeyCode::PageUp => todo!(),
                // crossterm::event::KeyCode::PageDown => todo!(),
                // crossterm::event::KeyCode::BackTab => todo!(),
                // crossterm::event::KeyCode::Delete => todo!(),
                // crossterm::event::KeyCode::Insert => todo!(),
                // crossterm::event::KeyCode::F(_) => todo!(),
                // crossterm::event::KeyCode::Null => todo!(),
                // crossterm::event::KeyCode::CapsLock => todo!(),
                // crossterm::event::KeyCode::ScrollLock => todo!(),
                // crossterm::event::KeyCode::NumLock => todo!(),
                // crossterm::event::KeyCode::PrintScreen => todo!(),
                // crossterm::event::KeyCode::Pause => todo!(),
                // crossterm::event::KeyCode::Menu => todo!(),
                // crossterm::event::KeyCode::KeypadBegin => todo!(),
                // crossterm::event::KeyCode::Media(media_key_code) => todo!(),
                // crossterm::event::KeyCode::Modifier(modifier_key_code) => todo!(),
                _ => {}
            }

            terminal.draw(|frame| AppAuth::render(frame, is_name, &name, &server))?;
        }

        Err(anyhow!("Failed to auth on loop"))
    }

    fn render(frame: &mut Frame, is_name: bool, name: &[char], server: &[char]) {
        let mut text = Text::from(Line::from("Select display name and server to join").bold());

        let (name_pref, status_pref) = if is_name {
            (Span::from("> ").blue(), Span::from("  "))
        } else {
            (Span::from("  "), Span::from("> ").blue())
        };

        text.push_line("");
        text.push_line(Line::from(vec![
            Span::from("Display Name"),
            Span::from(" *").light_red(),
            Span::from(":"),
        ]));
        text.push_line(Line::from(vec![
            name_pref,
            Span::from(String::from_iter(name)),
        ]));

        text.push_line("");
        text.push_line("Server ( Leave blank for main server ):");
        text.push_line(Line::from(vec![
            status_pref,
            Span::from(String::from_iter(server)),
        ]));

        text.push_line("");
        text.push_line("Press Enter to try to connect");

        frame.render_widget(
            Paragraph::new(text).block(
                Block::bordered()
                    .border_style(Style::new().light_blue())
                    .title_top(Line::from(" Tempest ~ Auth").bold().white())
                    .title_bottom(Line::from(" Esc to quit ").bold().white().right_aligned()),
            ),
            frame.area(),
        )
    }

    fn render_loading(frame: &mut Frame) {
        frame.render_widget(
            Paragraph::new(Text::from(Line::from("Waiting for authentication").bold())).block(
                Block::bordered()
                    .border_style(Style::new().light_blue())
                    .title_top(Line::from(" Tempest ~ Auth").bold().white())
                    .title_bottom(Line::from(" Esc to quit ").bold().white().right_aligned()),
            ),
            frame.area(),
        )
    }

    async fn try_connect(
        name: String,
        server: String,
    ) -> anyhow::Result<(
        AppLobby,
        TcpSender<ClientMessage>,
        TcpReceiver<ServerMessage>,
    )> {
        let addr = if server.is_empty() {
            "127.0.0.1:9000".to_string()
        } else {
            server
        };

        let (mut sender, mut receiver) =
            split_stream::<ClientMessage, ServerMessage>(TcpStream::connect(addr).await?);

        sender
            .send(&ClientMessage::Authenticate(name.clone()))
            .await?;

        let id = Self::wait_for_auth(&mut receiver).await?;
        let lobby_state = Self::wait_for_lobby_state(&mut receiver).await?;

        Ok((AppLobby::new(name, id, lobby_state), sender, receiver))
    }

    async fn wait_for_auth(receiver: &mut TcpReceiver<ServerMessage>) -> anyhow::Result<u32> {
        while let Some(msg) = receiver.next_message().await {
            let (msg, _) = msg?;

            if let ServerMessage::AuthResponse(id) = msg {
                return Ok(id);
            } else {
                println!("Received pointless command {msg:?}");
            }
        }

        Err(anyhow!("Failed to receive id"))
    }

    async fn wait_for_lobby_state(
        receiver: &mut TcpReceiver<ServerMessage>,
    ) -> anyhow::Result<ClientLobbyState> {
        while let Some(msg) = receiver.next_message().await {
            let (msg, _) = msg?;

            if let ServerMessage::LobbyState(state) = msg {
                return Ok(state);
            } else {
                println!("Received pointless command {msg:?}");
            }
        }

        Err(anyhow!("Failed to receive lobby state"))
    }
}
