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

#[mockall_double::double]
use crate::node::connection::ConnectionHandle;
use crate::node::connection::{ConnectionResult, ConnectionResultSender};
use crate::node::protocol::Message;
use futures::Future;
use mockall::automock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::error::Error;
use std::fmt::Debug;
use tokio::sync::{mpsc, oneshot};
use tokio_util::bytes::Bytes;

pub mod service;

#[derive(Debug)]
pub(crate) enum ReliableMessage {
    Send {
        message: Message,                   // Message to send
        respond_to: ConnectionResultSender, // oneshot channel to send ACK or Failure to
    },
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub enum ReliableNetworkMessage {
    Send(Message, u64),
    Ack(u64),
}

/// Methods for all protocol messages
impl ReliableNetworkMessage {
    /// Return the message as bytes
    pub fn as_bytes(&self) -> Option<Bytes> {
        let mut s = flexbuffers::FlexbufferSerializer::new();
        self.serialize(&mut s).unwrap();
        Some(Bytes::from(s.take_buffer()))
    }

    /// Build message from bytes
    pub fn from_bytes(b: &[u8]) -> Result<Self, Box<dyn Error>> {
        Ok(flexbuffers::from_slice(b)?)
    }
}

type AckWaiter = (Message, ConnectionResultSender);

struct ReliableSenderActor {
    receiver: mpsc::Receiver<ReliableMessage>,
    connection_handle: ConnectionHandle,
    waiting_for_ack: HashMap<u64, AckWaiter>,
    sequence_number: u64,
    connection_receiver: mpsc::Receiver<ReliableNetworkMessage>,
    application_sender: mpsc::Sender<Message>,
}

impl ReliableSenderActor {
    fn new(
        receiver: mpsc::Receiver<ReliableMessage>,
        connection_handle: ConnectionHandle,
        connection_receiver: mpsc::Receiver<ReliableNetworkMessage>,
        application_sender: mpsc::Sender<Message>,
    ) -> Self {
        ReliableSenderActor {
            receiver,
            connection_handle,
            waiting_for_ack: HashMap::new(),
            sequence_number: 0,
            connection_receiver,
            application_sender,
        }
    }

    fn next_sequence_number(&mut self) -> u64 {
        self.sequence_number += 1;
        self.sequence_number
    }

    async fn handle_message(
        &mut self,
        msg: ReliableMessage,
    ) -> Result<(), Box<dyn std::error::Error>> {
        match msg {
            ReliableMessage::Send {
                message,
                respond_to,
            } => {
                let sequence_number = self.next_sequence_number();
                self.waiting_for_ack
                    .insert(sequence_number, (message.clone(), respond_to));
                let reliable_message = ReliableNetworkMessage::Send(message, sequence_number);
                let _ = self.connection_handle.send(reliable_message).await;
                // Return success from here, the handle will still wait for message on respond_to which is in the waiting_for_ack map
                Ok(())
            }
        }
    }

    /// Handle a message received from the network.
    /// If it is a Send type, send an Ack back
    /// If it is an Ack, then remove the acked message from waiting_for_ack and return Ok() to application waiting
    async fn handle_connection_message(
        &mut self,
        msg: ReliableNetworkMessage,
    ) -> Result<(), Box<dyn std::error::Error>> {
        match msg {
            ReliableNetworkMessage::Send(message, sequence_number) => {
                // send ack back to message sender
                if let Err(e) = self
                    .connection_handle
                    .send(ReliableNetworkMessage::Ack(sequence_number))
                    .await
                {
                    log::info!("Error sending ack {}", e);
                    return Err("Error sending ack".into());
                }
                // send message up to the application
                if let Err(e) = self.application_sender.send(message).await {
                    log::info!("Error sending message to application. {}", e);
                }
            }
            ReliableNetworkMessage::Ack(sequence_number) => {
                match self.waiting_for_ack.remove(&sequence_number) {
                    Some((_msg, sender)) => {
                        log::debug!("Received ACK {}", sequence_number);
                        if let Err(e) = sender.send(Ok(())) {
                            log::info!("Error sending OK back to application {:?}", e);
                        }
                    }
                    None => {
                        log::debug!("No message waiting for the ACK received");
                    }
                }
            }
        }
        Ok(())
    }
}

async fn start_reliable_sender(mut actor: ReliableSenderActor) {
    loop {
        tokio::select! {
            Some(msg) = actor.receiver.recv() => {
                if let Err(e) = actor.handle_message(msg).await {
                    log::info!("Error handling message from application. Shutting down. {}", e);
                }
            },
            Some(msg) = actor.connection_receiver.recv() => {
                log::debug!("Received message from connection {:?}", msg);
                if let Err(e) = actor.handle_connection_message(msg).await {
                    log::info!("Error handling received message. Shutting down. {}", e);
                }
            },
            else => {
                log::info!("Bad message for reliable sender actor");
            }
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ReliableSenderHandle {
    pub(crate) sender: mpsc::Sender<ReliableMessage>,
}

#[automock]
pub trait ReliableSender {
    fn send(&self, message: Message) -> impl Future<Output = ConnectionResult<()>> + Send;
}

impl ReliableSenderHandle {
    pub async fn start(
        connection_handle: ConnectionHandle,
        connection_receiver: mpsc::Receiver<ReliableNetworkMessage>,
    ) -> (mpsc::Receiver<Message>, Self) {
        let (sender, receiver) = mpsc::channel(32);
        let (application_sender, application_receiver) = mpsc::channel(32);

        let actor = ReliableSenderActor::new(
            receiver,
            connection_handle,
            connection_receiver,
            application_sender,
        );
        tokio::spawn(start_reliable_sender(actor));

        (application_receiver, ReliableSenderHandle { sender })
    }
}

impl ReliableSender for ReliableSenderHandle {
    /// Send a message reliably.
    fn send(&self, message: Message) -> impl Future<Output = ConnectionResult<()>> + Send {
        Box::pin(async move {
            let (sender_from_actor, _receiver_from_actor) = oneshot::channel();
            let msg = ReliableMessage::Send {
                message,
                respond_to: sender_from_actor,
            };
            match self.sender.send(msg).await {
                Ok(_) => Ok(()),
                _ => Err("Error".into()),
            }
        })
    }
}

#[cfg(test)]
mod reliable_sender_tests {
    use super::ReliableNetworkMessage;
    use crate::node::connection::MockConnectionHandle;
    use crate::node::protocol::{Message, PingMessage};
    use serde::Serialize;
    use tokio::sync::mpsc;
    use tokio_util::bytes::Bytes;

    use super::{ReliableSender, ReliableSenderHandle};

    #[test]
    fn it_serializes_ping_message() {
        let ping_reliable_message = ReliableNetworkMessage::Send(
            Message::Ping(PingMessage {
                sender_id: "localhost".to_string(),
                message: String::from("ping"),
            }),
            1,
        );
        let mut s = flexbuffers::FlexbufferSerializer::new();
        ping_reliable_message.serialize(&mut s).unwrap();
        let b = Bytes::from(s.take_buffer());

        let msg = ReliableNetworkMessage::from_bytes(&b).unwrap();
        assert_eq!(msg, ping_reliable_message);
    }

    #[test]
    fn it_serializes_ping_message_using_as_bytes() {
        let ping_reliable_message = ReliableNetworkMessage::Send(
            Message::Ping(PingMessage {
                sender_id: "localhost".to_string(),
                message: String::from("ping"),
            }),
            1,
        );
        let serialized = ping_reliable_message.as_bytes().unwrap();

        let msg = ReliableNetworkMessage::from_bytes(&serialized).unwrap();
        assert_eq!(msg, ping_reliable_message);
    }

    #[tokio::test]
    async fn it_should_successfully_send_message_to_actor_and_receieve_an_ack() {
        let (connection_sender, connection_receiver) = mpsc::channel(32);
        let mut mock_connection_handle = MockConnectionHandle::default();
        mock_connection_handle.expect_send().return_once(|_| Ok(()));

        let (_application_receiver, reliable_sender_handler) =
            ReliableSenderHandle::start(mock_connection_handle, connection_receiver).await;

        let message = Message::Ping(PingMessage {
            sender_id: "localhost".to_string(),
            message: "ping".to_string(),
        });

        let ack_task = tokio::spawn(async move {
            let _ = connection_sender.send(ReliableNetworkMessage::Ack(1)).await;
        });

        let send_task = reliable_sender_handler.send(message);

        let (send_result, _) = tokio::join!(send_task, ack_task);
        assert!(send_result.is_ok());
    }

    #[tokio::test]
    async fn it_should_successfully_send_ack_when_message_received() {
        let (connection_sender, connection_receiver) = mpsc::channel(32);
        let mut mock_connection_handle = MockConnectionHandle::default();
        mock_connection_handle.expect_send().return_once(|_| Ok(()));

        let (mut application_receiver, _reliable_sender_handler) =
            ReliableSenderHandle::start(mock_connection_handle, connection_receiver).await;

        let message = Message::Ping(PingMessage {
            sender_id: "localhost".to_string(),
            message: "ping".to_string(),
        });

        let _ = connection_sender
            .send(ReliableNetworkMessage::Send(message.clone(), 1))
            .await;

        let received = application_receiver.recv().await;
        assert_eq!(received, Some(message));
    }

    #[tokio::test]
    async fn it_should_handle_error_on_message_received() {
        let (connection_sender, connection_receiver) = mpsc::channel(32);
        let mut mock_connection_handle = MockConnectionHandle::default();
        mock_connection_handle
            .expect_send()
            .return_once(|_| Err("Some error".into()));

        let (mut application_receiver, _reliable_sender_handler) =
            ReliableSenderHandle::start(mock_connection_handle, connection_receiver).await;

        let message = Message::Ping(PingMessage {
            sender_id: "localhost".to_string(),
            message: "ping".to_string(),
        });

        let _ = connection_sender
            .send(ReliableNetworkMessage::Send(message.clone(), 1))
            .await;

        let result = application_receiver.try_recv();
        assert!(result.is_err());
    }
}
