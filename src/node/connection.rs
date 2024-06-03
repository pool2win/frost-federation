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

use tokio::sync::oneshot::error::RecvError;
use tokio::{
    net::{
        tcp::{OwnedReadHalf, OwnedWriteHalf},
        TcpStream,
    },
    sync::{mpsc, oneshot},
};
use tokio_util::{
    bytes::Bytes,
    codec::{FramedRead, FramedWrite, LengthDelimitedCodec},
};
// Bring StreamExt in scope for access to `next` calls
use tokio_stream::StreamExt;
// Bring SinkExt in scope for access to `send` calls
use futures::sink::SinkExt;

use super::noise_handler::NoiseHandler;

#[derive(Debug)]
pub enum ConnectionMessage {
    Send {
        data: Bytes,
        respond_to: oneshot::Sender<()>,
    },
    SendClearText {
        data: Bytes,
        respond_to: oneshot::Sender<()>,
    },
    Subscribe {
        respond_to: mpsc::Sender<Bytes>,
    },
}

#[derive(Debug)]
pub struct ConnectionActor {
    reader: FramedRead<OwnedReadHalf, LengthDelimitedCodec>,
    writer: FramedWrite<OwnedWriteHalf, LengthDelimitedCodec>,
    receiver: mpsc::Receiver<ConnectionMessage>,
    subscribers: Vec<mpsc::Sender<Bytes>>,
    noise: NoiseHandler,
}

impl ConnectionActor {
    /// Build a new connection struct to work with the TcpStream
    pub async fn start(
        stream: TcpStream,
        receiver: mpsc::Receiver<ConnectionMessage>,
        subscription_sender: mpsc::Sender<Bytes>,
        key: String,
        init: bool,
    ) -> Self {
        // Set up a length delimted codec for Noise
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
            noise.run_handshake(framed_reader, framed_writer).await;
        noise.start_transport();
        log::debug!("Noise transport started");

        ConnectionActor {
            reader: framed_reader,
            writer: framed_writer,
            receiver,
            subscribers: vec![subscription_sender],
            noise,
        }
    }

    pub async fn handle_message(&mut self, msg: ConnectionMessage) {
        match msg {
            ConnectionMessage::Send { data, respond_to } => {
                self.handle_send(data, respond_to).await
            }
            ConnectionMessage::SendClearText { data, respond_to } => {
                self.handle_send_clear_text(data, respond_to).await
            }
            ConnectionMessage::Subscribe { respond_to } => self.handle_subscribe(respond_to).await,
        }
    }

    pub async fn handle_send(&mut self, data: Bytes, respond_to: oneshot::Sender<()>) {
        log::debug!("handle_send {:?}", data);
        let data = self.noise.build_transport_message(&data);
        if self.writer.send(data).await.is_err() {
            log::info!("Closing connection");
            let _ = self.writer.close().await;
            let _ = respond_to.send(());
        }
    }

    pub async fn handle_send_clear_text(&mut self, data: Bytes, respond_to: oneshot::Sender<()>) {
        if self.writer.send(data).await.is_err() {
            log::info!("Closing connection");
            let _ = respond_to.send(());
        }
    }

    pub async fn handle_subscribe(&mut self, respond_to: mpsc::Sender<Bytes>) {
        self.subscribers.push(respond_to);
    }

    pub async fn update_subscribers(&mut self, message: Bytes) {
        log::debug!("Updating subscribers... {}", self.subscribers.len());
        let decrypted_message = self.noise.read_transport_message(message);
        for subscriber in &self.subscribers {
            let _ = subscriber.send(decrypted_message.clone()).await;
        }
    }
}

pub async fn run_connection_actor(mut actor: ConnectionActor) {
    loop {
        tokio::select! {
            Some(message) = actor.receiver.recv() => { // read next command from handle
                actor.handle_message(message).await;
            }
            message = actor.reader.next() => { // read next message from network
                match message {
                    Some(message) => {
                        let msg = message.unwrap().freeze();
                        log::debug!("Received from network {:?}", msg.clone());
                        actor.update_subscribers(msg.clone()).await;
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
    ) -> (Self, mpsc::Receiver<Bytes>) {
        let (sender, receiver) = mpsc::channel(32);
        let (subscription_sender, subscription_receiver) = mpsc::channel(32);

        let connection_actor =
            ConnectionActor::start(tcp_stream, receiver, subscription_sender, key, init).await;
        tokio::spawn(run_connection_actor(connection_actor));

        (Self { sender }, subscription_receiver)
    }

    pub async fn send(&self, data: Bytes) -> Result<(), RecvError> {
        log::debug!("Send {:?}", data);
        let (sender, receiver) = oneshot::channel();
        let msg = ConnectionMessage::Send {
            data,
            respond_to: sender,
        };
        let _ = self.sender.send(msg).await;
        receiver.await
    }

    pub async fn send_clear_text(&self, data: Bytes) -> Result<(), RecvError> {
        let (sender, receiver) = oneshot::channel();
        let msg = ConnectionMessage::SendClearText {
            data,
            respond_to: sender,
        };
        let _ = self.sender.send(msg).await;
        receiver.await
    }

    pub async fn add_subscription(&self) -> mpsc::Receiver<Bytes> {
        let (subscription_sender, subscription_receiver) = mpsc::channel(32);
        let msg = ConnectionMessage::Subscribe {
            respond_to: subscription_sender,
        };

        let _ = self.sender.send(msg).await;
        subscription_receiver
    }
}
