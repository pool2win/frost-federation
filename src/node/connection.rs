// Copyright 2024 Kulpreet Singh

// This file is part of Frost-Federation

// Frost-Federation is free software: you can redistribute it and/or
// modify it under the terms of the GNU General Public License as
// published by the Free Software Foundation, either version 3 of the
// License, or (at your option) any later version.

// Frost-Federation is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU
// General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Frost-Federation. If not, see
// <https://www.gnu.org/licenses/>.

use super::{
    noise_handler::{run_handshake, NoiseHandler, NoiseIO},
    reliable_sender::ReliableNetworkMessage,
};
use futures::sink::SinkExt; // Bring SinkExt in scope for access to `send` calls
use tokio::{
    net::{
        tcp::{OwnedReadHalf, OwnedWriteHalf},
        TcpStream,
    },
    sync::{mpsc, oneshot},
};
use tokio_stream::StreamExt; // Bring StreamExt in scope for access to `next` calls
use tokio_util::{
    bytes::Bytes,
    codec::{FramedRead, FramedWrite, LengthDelimitedCodec},
};

pub(crate) type ConnectionReader = FramedRead<OwnedReadHalf, LengthDelimitedCodec>;
pub(crate) type ConnectionWriter = FramedWrite<OwnedWriteHalf, LengthDelimitedCodec>;
type ConnectionError = Box<dyn std::error::Error + Send + Sync + 'static>;
pub(crate) type ConnectionResult<T> = std::result::Result<T, ConnectionError>;
pub(crate) type ConnectionResultSender = oneshot::Sender<ConnectionResult<()>>;

#[derive(Debug)]
pub enum ConnectionMessage {
    Send {
        data: ReliableNetworkMessage,
        respond_to: ConnectionResultSender,
    },
    SendClearText {
        data: ReliableNetworkMessage,
        respond_to: ConnectionResultSender,
    },
}

#[derive(Debug)]
pub struct ConnectionActor {
    reader: ConnectionReader,
    writer: ConnectionWriter,
    receiver: mpsc::Receiver<ConnectionMessage>,
    subscriber: mpsc::Sender<ReliableNetworkMessage>,
    noise: NoiseHandler,
}

impl ConnectionActor {
    /// Build a new connection struct to work with the TcpStream
    pub async fn start(
        stream: TcpStream,
        receiver: mpsc::Receiver<ConnectionMessage>,
        subscription_sender: mpsc::Sender<ReliableNetworkMessage>,
        key: String,
        init: bool,
    ) -> Self {
        // Set up a length delimted codec
        let (r, w) = stream.into_split();
        let framed_reader = LengthDelimitedCodec::builder()
            .length_field_offset(0)
            .length_field_length(2)
            .length_adjustment(0)
            .new_read(r);
        let framed_writer = LengthDelimitedCodec::builder()
            .length_field_offset(0)
            .length_field_length(2)
            .length_adjustment(0)
            .new_write(w);

        let mut noise = NoiseHandler::new(init, key);
        let (framed_reader, framed_writer) =
            run_handshake(&mut noise, init, framed_reader, framed_writer).await;
        log::debug!("Noise transport started");

        ConnectionActor {
            reader: framed_reader,
            writer: framed_writer,
            receiver,
            subscriber: subscription_sender,
            noise,
        }
    }

    pub async fn handle_message(
        &mut self,
        msg: ConnectionMessage,
    ) -> Result<(), Box<dyn std::error::Error>> {
        match msg {
            ConnectionMessage::Send { data, respond_to } => {
                self.handle_send(data, respond_to).await
            }
            ConnectionMessage::SendClearText { data, respond_to } => {
                self.handle_send_clear_text(data, respond_to).await
            }
        }
    }

    pub async fn handle_send(
        &mut self,
        msg: ReliableNetworkMessage,
        respond_to: ConnectionResultSender,
    ) -> Result<(), Box<dyn std::error::Error>> {
        log::debug!("handle_send {:?}", msg);
        match msg.as_bytes() {
            Some(network_message) => {
                let data = self.noise.build_transport_message(&network_message);
                match self.writer.send(data).await {
                    Err(_) => {
                        log::info!("Closing connection");
                        let _ = self.writer.close().await;
                        Err("Error writing to socket stream".into())
                    }
                    Ok(_) => {
                        let _ = respond_to.send(Ok(()));
                        Ok(())
                    }
                }
            }
            None => Err("Error serializing message".into()),
        }
    }

    pub async fn handle_send_clear_text(
        &mut self,
        msg: ReliableNetworkMessage,
        respond_to: ConnectionResultSender,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(data) = msg.as_bytes() {
            self.writer.send(data).await?;
            match respond_to.send(Ok(())) {
                Ok(_) => Ok(()),
                Err(_) => {
                    Err("Unable to write to client sending clear text. Client went away?".into())
                }
            }
        } else {
            Err("Error serializing message".into())
        }
    }

    pub async fn handle_received(
        &mut self,
        message: Bytes,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let decrypted_message = self.noise.read_transport_message(message);
        let network_message = ReliableNetworkMessage::from_bytes(&decrypted_message)?;
        log::debug!("Received message {:?}", network_message);
        self.subscriber.send(network_message).await?;
        Ok(())
    }
}

pub async fn run_connection_actor(mut actor: ConnectionActor) {
    loop {
        tokio::select! {
            Some(message) = actor.receiver.recv() => { // read next command from handle
                // TODO: Enable concurrent processing of messages received on a connection. Currently we handle one message at a time.
                if actor.handle_message(message).await.is_err() {
                    log::info!("Connection actor reader closed");
                    return;
                }
            }
            message = actor.reader.next() => { // read next message from network
                match message {
                    Some(message) => {
                        if message.is_err() {
                            log::info!("Connection closed by peer");
                            return
                        }
                        let msg = message.unwrap().freeze();
                        log::debug!("Received from network {:?}", msg.clone());
                        actor.handle_received(msg.clone()).await.unwrap();
                    },
                    None => { // Stream closed, return to clear up connection
                        log::debug!("Connection actor reader closed");
                        return;
                    }
                }
            }
            else => {
                log::debug!("Connection actor stopping");
                return;
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct ConnectionHandle {
    sender: mpsc::Sender<ConnectionMessage>,
}

impl ConnectionHandle {
    pub async fn start(
        tcp_stream: TcpStream,
        key: String,
        init: bool,
    ) -> (Self, mpsc::Receiver<ReliableNetworkMessage>) {
        let (sender, receiver) = mpsc::channel(32);
        let (subscription_sender, subscription_receiver) = mpsc::channel(32);

        let connection_actor =
            ConnectionActor::start(tcp_stream, receiver, subscription_sender, key, init).await;
        tokio::spawn(run_connection_actor(connection_actor));

        (Self { sender }, subscription_receiver)
    }

    pub async fn send(&self, data: ReliableNetworkMessage) -> ConnectionResult<()> {
        log::debug!("Send {:?}", data);
        let (sender, receiver) = oneshot::channel();
        let msg = ConnectionMessage::Send {
            data,
            respond_to: sender,
        };
        let _ = self.sender.send(msg).await;
        receiver.await?
    }
}

mockall::mock! {
    pub ConnectionHandle {
        pub async fn start(tcp_stream: TcpStream, key: String, init: bool,) -> (Self, mpsc::Receiver<ReliableNetworkMessage>);
        pub async fn send(&self, data: ReliableNetworkMessage) -> ConnectionResult<()>;
    }
}
