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

use super::noise_handler::NoiseIO;
use super::{noise_handler::handshake::run_handshake, reliable_sender::ReliableNetworkMessage};
use futures::sink::SinkExt; // Bring SinkExt in scope for access to `send` calls
use tokio::{
    io::{AsyncRead, AsyncWrite},
    sync::{mpsc, oneshot},
};
use tokio_stream::StreamExt; // Bring StreamExt in scope for access to `next` calls
use tokio_util::{
    bytes::Bytes,
    codec::{FramedRead, FramedWrite, LengthDelimitedCodec},
};

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
pub struct ConnectionActor<R, W, N> {
    reader: FramedRead<R, LengthDelimitedCodec>,
    writer: FramedWrite<W, LengthDelimitedCodec>,
    receiver: mpsc::Receiver<ConnectionMessage>,
    subscriber: mpsc::Sender<ReliableNetworkMessage>,
    noise: N,
}

impl<R, W, N> ConnectionActor<R, W, N>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
    N: NoiseIO,
{
    pub fn build_reader_writer(
        reader: R,
        writer: W,
    ) -> (
        FramedRead<R, LengthDelimitedCodec>,
        FramedWrite<W, LengthDelimitedCodec>,
    ) {
        let framed_reader = LengthDelimitedCodec::builder()
            .length_field_offset(0)
            .length_field_length(2)
            .length_adjustment(0)
            .new_read(reader);
        let framed_writer = LengthDelimitedCodec::builder()
            .length_field_offset(0)
            .length_field_length(2)
            .length_adjustment(0)
            .new_write(writer);
        (framed_reader, framed_writer)
    }

    /// Build a new connection struct to work with the TcpStream
    pub async fn start(
        reader: R,
        writer: W,
        mut noise: N,
        receiver: mpsc::Receiver<ConnectionMessage>,
        subscription_sender: mpsc::Sender<ReliableNetworkMessage>,
        init: bool,
    ) -> Self {
        // Set up a length delimted codec
        let (mut framed_reader, mut framed_writer) = Self::build_reader_writer(reader, writer);
        run_handshake(&mut noise, init, &mut framed_reader, &mut framed_writer).await;
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

pub async fn run_connection_actor<R, W, N>(mut actor: ConnectionActor<R, W, N>)
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
    N: NoiseIO,
{
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
    pub async fn start<R, W, N>(
        reader: R,
        writer: W,
        noise: N,
        init: bool,
    ) -> (Self, mpsc::Receiver<ReliableNetworkMessage>)
    where
        R: AsyncRead + Unpin + Send + 'static,
        W: AsyncWrite + Unpin + Send + 'static,
        N: NoiseIO + Send + 'static,
    {
        let (sender, receiver) = mpsc::channel(32);
        let (subscription_sender, subscription_receiver) = mpsc::channel(32);

        let connection_actor =
            ConnectionActor::start(reader, writer, noise, receiver, subscription_sender, init)
                .await;
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
        pub async fn start<R, W, N>(reader: R, writer: W, noise: N, init: bool) -> (Self, mpsc::Receiver<ReliableNetworkMessage>)
        where R: AsyncRead + Unpin + Send + 'static,
              W: AsyncWrite + Unpin + Send + 'static,
              N: NoiseIO + Send + 'static;
        pub async fn send(&self, data: ReliableNetworkMessage) -> ConnectionResult<()>;
    }
}

#[cfg(test)]
mod tests {
    use super::ConnectionHandle;
    // use crate::node::protocol::{PingMessage, ProtocolMessage};
    // use crate::node::reliable_sender::ReliableNetworkMessage;
    use crate::node::noise_handler::MockNoiseIO;
    use tokio_test::io::Builder;
    use tokio_util::bytes::Bytes;

    #[tokio::test]
    async fn it_should_start_connection() {
        let async_reader = Builder::new().read(b"1").build();
        let async_writer = Builder::new().write(b"1").build();

        let mut noise = MockNoiseIO::default();
        noise
            .expect_build_handshake_message()
            .return_const(Bytes::from("1"));
        noise
            .expect_read_handshake_message()
            .return_const(Bytes::from("1"));
        let (_handle, mut _receiver) =
            ConnectionHandle::start(async_reader, async_writer, noise, true).await;
    }

    // #[tokio::test]
    // async fn it_should_start_connection_and_send_receive_message() {
    //     let async_reader = Builder::new().read(b"hello world").build();
    //     let async_writer = Builder::new().write(b"hello world").build();

    //     let (handle, mut receiver) =
    //         ConnectionHandle::start(async_reader, async_writer, "test key".to_string(), true).await;

    //     let msg = ReliableNetworkMessage::Send(PingMessage::start().unwrap(), 1);
    //     let _ = handle.send(msg).await;

    //     let received_msg = receiver.recv().await.unwrap();
    //     assert_eq!(
    //         received_msg,
    //         ReliableNetworkMessage::Send(PingMessage::start().unwrap(), 2)
    //     );
    // }
}
