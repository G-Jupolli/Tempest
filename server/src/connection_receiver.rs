use rpc::comms::{ClientMessage, ServerMessage, TcpReceiver, TcpSender, split_stream};
use std::{net::SocketAddr, time::Duration};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::mpsc::{self, UnboundedReceiver},
    time::sleep,
};

use crate::{AuthIntraMessage, ServerIntraMessage};

// This struct is to create a listener loop used to accept connections
// and register them in the main game server.
// This will also handle logic for user disconnections
pub struct ConnectionReceiver;

struct ConnectionNode;

const CONNECTION_TIMEOUT_INTERVAL: Duration = Duration::from_secs(30);

impl ConnectionReceiver {
    pub async fn start_listener(
        event_sender: mpsc::UnboundedSender<ServerIntraMessage>,
    ) -> anyhow::Result<()> {
        println!("Now try listen");
        let listener = TcpListener::bind("127.0.0.1:9000").await?;

        println!("Listening On 127.0.0.1:9000");

        loop {
            // This stream is to receive messages
            let (stream, remote_addr) = listener.accept().await?;

            ConnectionNode::handle_connection_node(stream, remote_addr, event_sender.clone());
        }
    }
}

impl ConnectionNode {
    fn handle_connection_node(
        stream: TcpStream,
        remote_addr: SocketAddr,
        event_sender: mpsc::UnboundedSender<ServerIntraMessage>,
    ) {
        tokio::spawn(async move {
            ConnectionNode::start_connection_node(stream, remote_addr, event_sender).await;
        });
    }

    async fn start_connection_node(
        stream: TcpStream,
        client_addr: SocketAddr,
        event_sender: mpsc::UnboundedSender<ServerIntraMessage>,
    ) {
        println!("Received client connection from {client_addr}");

        let (tcp_sender, mut tcp_receiver) = split_stream::<ServerMessage, ClientMessage>(stream);

        // On an incoming connection, we wait for the user to send auth
        // on failure or timeout we send down an error message and then close the connection
        let name = tokio::select! {
            _ = sleep(CONNECTION_TIMEOUT_INTERVAL) => {
                // let err = ServiceCommand::Error(400, "Auth Timeout".to_string());
                println!("Auth timeout on {client_addr}, close connection");
                None
            }
            name = Self::wait_for_name(&mut tcp_receiver) => {
                println!("Received Some name {name:?}");
                name.ok()
            }
        };

        let Some(name) = name else {
            println!("No name, close connection");
            return;
        };

        println!("Now have some name {name}");

        let (client_sender, sender_channel) = mpsc::unbounded_channel::<ServerMessage>();

        event_sender
            .send(ServerIntraMessage::RegisterUser(
                name,
                client_addr,
                client_sender,
            ))
            // REMOVE THIS AND HANDLE IT BETTER
            // REMOVE THIS AND HANDLE IT BETTER
            // REMOVE THIS AND HANDLE IT BETTER
            // REMOVE THIS AND HANDLE IT BETTER
            .unwrap();

        Self::start_sender_loop(tcp_sender, sender_channel);

        while let Some(Ok((msg, _))) = tcp_receiver.next_message().await {
            match msg {
                ClientMessage::Authenticate(name) => {
                    println!("Client sent authenticate for no reason with name {name}");
                }
                ClientMessage::Authed(user_id, client_authed_command) => {
                    event_sender
                        .send(ServerIntraMessage::Auth(AuthIntraMessage {
                            addr: client_addr,
                            user_id,
                            message: client_authed_command,
                        }))
                        // REMOVE THIS AND HANDLE IT BETTER
                        // REMOVE THIS AND HANDLE IT BETTER
                        // REMOVE THIS AND HANDLE IT BETTER
                        // REMOVE THIS AND HANDLE IT BETTER
                        .unwrap();
                }
            }
        }

        let _ = event_sender.send(ServerIntraMessage::Disconnected(client_addr));
    }

    async fn wait_for_name(receiver: &mut TcpReceiver<ClientMessage>) -> anyhow::Result<String> {
        while let Some(Ok((msg, _))) = receiver.next_message().await {
            if let ClientMessage::Authenticate(name) = msg {
                return Ok(name);
            } else {
                println!("Received pointless message at this point {msg:?}");
            };
        }

        Err(anyhow::anyhow!("Connection closed"))
    }

    fn start_sender_loop(
        mut sender: TcpSender<ServerMessage>,
        mut channel: UnboundedReceiver<ServerMessage>,
    ) {
        tokio::spawn(async move {
            while let Some(send_message) = channel.recv().await {
                let _ = sender.send(&send_message).await.inspect_err(|err| {
                    println!("CRITICAL ERROR, failed to send message to client, {err:?}")
                });
            }
        });
    }

    // fn start_receiver_loop(
    //     mut receiver: TcpReceiver<ClientMessage>,
    //     mut intra_sender: UnboundedSender<ServerIntraMessage>,
    //     client_addr: SocketAddr,
    // ) {
    //     tokio::spawn(async move {
    //     });
    // }

    // async fn loop_on_node(mut receiver: TcpReceiver) {}
}
