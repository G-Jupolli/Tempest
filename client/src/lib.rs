use std::net::SocketAddr;

use futures::{
    StreamExt,
    stream::{SplitSink, SplitStream},
};
use rpc::comms::split_stream;
use tokio::{net::TcpStream, sync::mpsc};
use tokio_util::{
    bytes::Bytes,
    codec::{Framed, LengthDelimitedCodec},
};

pub struct TempestClient {
    channel: mpsc::UnboundedSender<String>,
    front_end: mpsc::UnboundedSender<String>,
}

/// So, I kind of want to make this client such that it can be used by
///   different front ends. In an Ideal world I would have an implementation
///   that supports: CLI, Web & native desktop app.
///
/// I'll start with the CLI seeing as that one is the easiest to integrate.
///
/// CLI :
/// Most likely use Ratatui and just build views from there
///
/// Web :
///  If I can have some sort of internal server or if I can wasm bingen this
///  I'll try that.
///  I'd rather not have to figure how to setup raw TCP streams via js, either way
///    I'd need to wasm bingen as this would need a translation layer for the structs.
///
/// Desktop app :
///  This is more of something I would like to have.
///  I don't necessarily care about this.
///
///  I could potentially wait until gpui is a bit more mature ( Not sure if it works on non unix ).
///  Realistically I could use piston but I don't think I want to work with game engines just yet.
///
impl TempestClient {
    pub async fn new(
        // I'm going to let people self host this
        server: SocketAddr,
        name: String,
        front_end: mpsc::UnboundedSender<String>,
    ) -> anyhow::Result<TempestClient> {
        let connection = TcpStream::connect(server)
            .await
            .inspect_err(|_| println!("Failed to connect to server"))?;

        // let (client_sender, client_receiver) = split_stream(connection);

        // client_sender.send_message(rpc::comms::Header::User, "Boop".to_string().into());

        // First we want to send the authentication command

        todo!()
    }
}
