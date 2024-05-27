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

#[derive(Debug)]
pub enum ConnectionMessage {
    Send {
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
}

impl ConnectionActor {
    /// Build a new connection struct to work with the TcpStream
    pub fn new(stream: TcpStream, receiver: mpsc::Receiver<ConnectionMessage>) -> Self {
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

        ConnectionActor {
            reader: framed_reader,
            writer: framed_writer,
            receiver,
        }
    }

    pub async fn handle_message(&mut self, msg: ConnectionMessage) {
        match msg {
            ConnectionMessage::Send { data, respond_to } => {
                self.handle_send(data, respond_to).await
            }
            ConnectionMessage::Subscribe { respond_to } => self.handle_subscribe(respond_to).await,
        }
    }

    pub async fn handle_send(&mut self, data: Bytes, respond_to: oneshot::Sender<()>) {
        // Bring SinkExt in scope for access to `send` calls
        use futures::sink::SinkExt;

        if self.writer.send(data).await.is_err() {
            log::info!("Closing connection");
            let _ = respond_to.send(());
        }
    }

    pub async fn handle_subscribe(&mut self, respond_to: mpsc::Sender<Bytes>) {
        // Bring StreamExt in scope for access to `next` calls
        use tokio_stream::StreamExt;

        while let Some(data) = self.reader.next().await {
            match data {
                Ok(msg) => {
                    let _ = respond_to.send(msg.freeze()).await;
                }
                Err(_) => {
                    log::info!("Error sending message to subscriber");
                    return;
                }
            }
        }
    }
}

pub async fn run_connection_actor(mut actor: ConnectionActor) {
    while let Some(msg) = actor.receiver.recv().await {
        actor.handle_message(msg).await;
    }
}

#[derive(Clone, Debug)]
pub struct ConnectionHandle {
    sender: mpsc::Sender<ConnectionMessage>,
}

impl ConnectionHandle {
    pub fn new(tcp_stream: TcpStream) -> Self {
        let (sender, receiver) = mpsc::channel(32);
        let connection_actor = ConnectionActor::new(tcp_stream, receiver);
        tokio::spawn(run_connection_actor(connection_actor));

        Self { sender }
    }

    pub async fn send(&self, data: Bytes) -> Result<(), RecvError> {
        let (sender, receiver) = oneshot::channel();
        let msg = ConnectionMessage::Send {
            data,
            respond_to: sender,
        };
        let _ = self.sender.send(msg).await;
        receiver.await
    }

    pub async fn start_subscription(&self) -> mpsc::Receiver<Bytes> {
        let (send, recv) = mpsc::channel(32);
        let msg = ConnectionMessage::Subscribe { respond_to: send };

        let _ = self.sender.send(msg).await;
        recv
    }
}
