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

use super::connection::{ConnectionResult, ConnectionResultSender};
use super::membership::ReliableSenderMap;
use super::protocol::message_id_generator::MessageId;
use super::protocol::message_id_generator::MessageIdGenerator;
use super::protocol::NetworkMessage;
use crate::node::protocol::Message;
#[mockall_double::double]
use crate::node::reliable_sender::ReliableSenderHandle;
use serde::Serialize;
use std::collections::HashMap;
use std::error::Error;
use tokio::sync::{mpsc, oneshot};

pub mod service;

type EchosMap = HashMap<MessageId, HashMap<String, bool>>;

/// Message types for echo broadcast actor -> handle communication
pub(crate) enum EchoBroadcastMessage {
    Send {
        data: Message,
        members: ReliableSenderMap,
        respond_to: ConnectionResultSender,
    },
    Echo {
        data: Message,
    },
}

/// Echo Broadcast Actor model implementation.
/// The actor receives messages from EchoBroadcastHandle
pub(crate) struct EchoBroadcastActor {
    /// A map from message id to the members in a map: message id -> [node id -> [reliable sender handles]]
    reliable_senders: HashMap<MessageId, ReliableSenderMap>,
    /// RX for actor to receive requests on
    command_receiver: mpsc::Receiver<EchoBroadcastMessage>,
    /// Map of echo messages received from the nodes in membership when this broadcast was initiated
    message_echos: EchosMap,
    /// TX for the message id's echo broadcast to finish
    message_client_txs: HashMap<MessageId, ConnectionResultSender>,
}

impl EchoBroadcastActor {
    pub fn start(receiver: mpsc::Receiver<EchoBroadcastMessage>) -> Self {
        Self {
            command_receiver: receiver,
            message_echos: EchosMap::new(),
            message_client_txs: HashMap::new(),
            reliable_senders: HashMap::new(),
        }
    }

    pub async fn handle_message(&mut self, message: EchoBroadcastMessage) {
        match message {
            EchoBroadcastMessage::Send {
                data,
                respond_to,
                members,
            } => {
                let _ = self.send_message(data, members, respond_to).await;
            }
            EchoBroadcastMessage::Echo { data } => {
                self.handle_received_echo(data).await;
            }
        }
    }

    /// Receives messages to echo broadcast, wait for echo messages
    /// from all members. Timeout is handled by layer on top.
    ///
    /// Process:
    /// 1. Wait for echos from all members for the message sent
    /// 2. On receiving echos from all members, return Ok() to waiting receiver
    pub async fn send_message(
        &mut self,
        data: Message,
        members: ReliableSenderMap,
        respond_to: ConnectionResultSender,
    ) -> Result<(), Box<dyn Error + Sync + Send>>
    where
        Message: NetworkMessage + Serialize,
    {
        let sender_id = data.get_sender_id();
        let message_id = data.get_message_id().unwrap();
        for (member, reliable_sender) in &members {
            if reliable_sender.send(data.clone()).await.is_err() {
                log::debug!("Error in reliable sender send...");
                let _ = respond_to.send(Err("Error sending using reliable sender".into()));
                return Err("Error sending message using reliable sender".into());
            }
            let message_echos = self.message_echos.entry(message_id.clone()).or_default();
            message_echos.entry(member.clone()).or_insert(false);
        }
        self.reliable_senders.insert(message_id.clone(), members);

        // Update sender_id's echo to true
        self.message_echos
            .get_mut(&message_id)
            .unwrap()
            .insert(sender_id, true);

        self.message_client_txs.insert(message_id, respond_to);
        Ok(())
    }

    /// Check if echos have been received from all members for a given message identifier
    pub fn echo_received_for_all(&self, message_id: &MessageId) -> bool {
        self.message_echos
            .get(message_id)
            .unwrap()
            .iter()
            .all(|(_sender_id, status)| *status)
    }

    /// Add received echo from a sender to the list of echos received
    pub fn add_echo(&mut self, message_id: &MessageId, sender_id: String) {
        match self.message_echos.get_mut(message_id) {
            Some(echos) => {
                echos
                    .entry(sender_id)
                    .and_modify(|v| *v = true)
                    .or_insert(true);
            }
            None => {
                self.message_echos.insert(
                    message_id.clone(),
                    HashMap::<String, bool>::from([(sender_id, true)]),
                );
            }
        }
    }

    /// Handle Echo messages received for this message
    pub async fn handle_received_echo(&mut self, data: Message)
    where
        Message: NetworkMessage + Serialize,
    {
        let sender_id = data.get_sender_id();
        let message_id = data.get_message_id().unwrap();

        self.add_echo(&message_id, sender_id.clone());

        if self.echo_received_for_all(&message_id) {
            match self.message_client_txs.remove(&message_id) {
                Some(respond_to) => {
                    if respond_to.send(Ok(())).is_err() {
                        log::error!("Error responding on echo broadcast completion");
                    }
                }
                None => {
                    log::error!("No receivers for the confirmed echo broadcast");
                }
            }
        }
    }
}

/// Handler for sending and confirming echo messages for a broadcast
/// Send using ReliableSenderHandle and then wait fro Echo message
///
/// Members list is copied into this struct so that we are only
/// waiting for echos from the parties that were members when the
/// broadcast was originally sent.
#[derive(Clone)]
pub(crate) struct EchoBroadcastHandle {
    sender: mpsc::Sender<EchoBroadcastMessage>,
}

/// Handle for the echo broadcast actor
impl EchoBroadcastHandle {
    /// Start the echo broadcast actor by listening to any messages on the
    /// receiver channel
    pub async fn start() -> Self {
        let (tx, rx) = mpsc::channel(512);
        let mut actor = EchoBroadcastActor::start(rx);
        tokio::spawn(async move {
            while let Some(message) = actor.command_receiver.recv().await {
                actor.handle_message(message).await;
            }
        });
        Self { sender: tx }
    }

    /// Keep the same signature to send, so we can convert that into a Trait later if we want.
    pub async fn send(&self, message: Message, members: ReliableSenderMap) -> ConnectionResult<()> {
        let (sender_from_actor, receiver_from_actor) = oneshot::channel();
        let msg = EchoBroadcastMessage::Send {
            data: message,
            members,
            respond_to: sender_from_actor,
        };
        if self.sender.send(msg).await.is_err() {
            log::debug!("Returning send error...");
            return Err("Connection error".into());
        }
        let result = receiver_from_actor.await;
        if result?.is_err() {
            Err("Broadcast error".into())
        } else {
            Ok(())
        }
    }

    /// Receive an echo broadcast message from connection
    /// Pass this to the actor, which will respond to the echo
    pub async fn receive(&self, message: Message, message_id: MessageId) -> ConnectionResult<()> {
        let msg = EchoBroadcastMessage::Echo { data: message };
        if self.sender.send(msg).await.is_err() {
            return Err("Failed to send receive to echo broadcast actor".into());
        }
        Ok(())
    }
}

mockall::mock! {
    pub EchoBroadcastHandle{
        pub async fn start() -> Self;
        pub async fn send(&self, message: Message, members: ReliableSenderMap) -> ConnectionResult<()>;
        pub async fn receive(&self, message: Message, message_id: MessageId) -> ConnectionResult<()>;
    }

    impl Clone for EchoBroadcastHandle {
        fn clone(&self) -> Self;
    }
}

#[cfg(test)]
mod echo_broadcast_actor_tests {
    use super::*;
    use crate::node::protocol::{Broadcast, RoundOnePackageMessage};
    use futures::FutureExt;

    #[tokio::test]
    async fn it_should_create_actor_with_echos_setup() {
        let (_sender, receiver) = mpsc::channel(32);
        let actor = EchoBroadcastActor::start(receiver);
        assert_eq!(actor.message_echos.keys().count(), 0);
    }

    #[tokio::test]
    async fn it_should_handle_send_message_with_ok_from_reliable_sender() {
        let (_sender, receiver) = mpsc::channel(32);
        let (respond_to, waiting_for_response) = oneshot::channel();

        let mut reliable_sender = ReliableSenderHandle::default();
        reliable_sender
            .expect_send()
            .return_once(|_| async { Ok(()) }.boxed());

        let reliable_senders_map = HashMap::from([("a".to_string(), reliable_sender)]);

        let mut actor = EchoBroadcastActor::start(receiver);

        let msg = Message::BroadcastMessage(Broadcast::RoundOnePackage(
            RoundOnePackageMessage {
                sender_id: "localhost".to_string(),
                message: "ping".to_string(),
            },
            Some(MessageId(1)),
        ));

        let result = actor
            .send_message(msg, reliable_senders_map, respond_to)
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn it_should_handle_send_message_with_error_from_reliable_sender() {
        let (_sender, receiver) = mpsc::channel(32);
        let (respond_to, waiting_for_response) = oneshot::channel();

        let mut reliable_sender = ReliableSenderHandle::default();
        reliable_sender
            .expect_send()
            .return_once(|_| async { Err("Some error".into()) }.boxed());

        let reliable_senders_map = HashMap::from([("a".to_string(), reliable_sender)]);

        let mut actor = EchoBroadcastActor::start(receiver);

        let msg = Message::BroadcastMessage(Broadcast::RoundOnePackage(
            RoundOnePackageMessage {
                sender_id: "localhost".to_string(),
                message: "ping".to_string(),
            },
            Some(MessageId(1)),
        ));

        let result = actor
            .send_message(msg, reliable_senders_map, respond_to)
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn it_should_send_handle_errors_if_reliable_sender_returns_error() {
        let mut first_reliable_sender_handle = ReliableSenderHandle::default();
        let mut second_reliable_sender_handle = ReliableSenderHandle::default();

        let msg = Message::BroadcastMessage(Broadcast::RoundOnePackage(
            RoundOnePackageMessage {
                sender_id: "localhost".to_string(),
                message: "ping".to_string(),
            },
            Some(MessageId(1)),
        ));

        first_reliable_sender_handle
            .expect_send()
            .return_once(|_| async { Ok(()) }.boxed());
        second_reliable_sender_handle
            .expect_send()
            .return_once(|_| async { Err("Error sending message".into()) }.boxed());

        let reliable_senders_map = HashMap::from([
            ("a".to_string(), first_reliable_sender_handle),
            ("b".to_string(), second_reliable_sender_handle),
        ]);
        let echo_bcast_handle = EchoBroadcastHandle::start().await;

        let result = echo_bcast_handle.send(msg, reliable_senders_map).await;
        assert!(result.is_err());
        assert_eq!(result.as_ref().unwrap_err().to_string(), "Broadcast error");
    }

    #[tokio::test]
    async fn it_should_handle_echo_message_if_waiting_for_echos() {
        let mut first_reliable_sender_handle = ReliableSenderHandle::default();
        let mut second_reliable_sender_handle = ReliableSenderHandle::default();

        first_reliable_sender_handle
            .expect_send()
            .return_once(|_| async { Ok(()) }.boxed());
        second_reliable_sender_handle
            .expect_send()
            .return_once(|_| async { Ok(()) }.boxed());

        let reliable_senders_map = HashMap::from([
            ("a".to_string(), first_reliable_sender_handle),
            ("b".to_string(), second_reliable_sender_handle),
        ]);

        let (_sender, receiver) = mpsc::channel(32);
        let mut echo_bcast_actor = EchoBroadcastActor::start(receiver);

        let msg = Message::BroadcastMessage(Broadcast::RoundOnePackage(
            RoundOnePackageMessage {
                sender_id: "localhost".to_string(),
                message: "ping".to_string(),
            },
            Some(MessageId(1)),
        ));

        let msg = EchoBroadcastMessage::Echo { data: msg };

        let _result = echo_bcast_actor.handle_message(msg).await;
        assert_eq!(echo_bcast_actor.message_echos.len(), 1);
        assert_eq!(
            echo_bcast_actor
                .message_echos
                .get(&MessageId(1))
                .unwrap()
                .len(),
            1
        );
        assert!(echo_bcast_actor
            .message_echos
            .get(&MessageId(1))
            .unwrap()
            .get("localhost")
            .is_some());
    }

    #[tokio::test]
    async fn it_should_add_echo_in_various_cases() {
        let (_sender, receiver) = mpsc::channel(32);
        let mut echo_bcast_actor = EchoBroadcastActor::start(receiver);

        echo_bcast_actor.add_echo(&MessageId(1), "b".to_string());
        echo_bcast_actor.add_echo(&MessageId(1), "c".to_string());

        assert_eq!(echo_bcast_actor.message_echos.len(), 1);
        assert_eq!(
            echo_bcast_actor
                .message_echos
                .get(&MessageId(1))
                .unwrap()
                .len(),
            2
        );

        // try to add same echo again
        echo_bcast_actor.add_echo(&MessageId(1), "b".to_string());
        assert_eq!(
            echo_bcast_actor
                .message_echos
                .get(&MessageId(1))
                .unwrap()
                .len(),
            2
        );
    }

    #[tokio::test]
    async fn it_should_handle_echo_before_all_received_will_add_echo() {
        let (_sender, receiver) = mpsc::channel(32);
        let mut echo_bcast_actor = EchoBroadcastActor::start(receiver);

        let msg = RoundOnePackageMessage {
            sender_id: "localhost".to_string(),
            message: "round one package".to_string(),
        };
        let broadcast_message =
            Message::BroadcastMessage(Broadcast::RoundOnePackage(msg, Some(MessageId(1))));

        echo_bcast_actor
            .handle_received_echo(broadcast_message)
            .await;

        assert_eq!(echo_bcast_actor.message_echos.len(), 1);
        assert_eq!(
            echo_bcast_actor
                .message_echos
                .get(&MessageId(1))
                .unwrap()
                .len(),
            1
        );
    }

    #[tokio::test]
    async fn it_should_handle_echo_to_manage_all_received_case() {
        let mut first_reliable_sender_handle = ReliableSenderHandle::default();
        let mut second_reliable_sender_handle = ReliableSenderHandle::default();

        first_reliable_sender_handle
            .expect_send()
            .return_once(|_| async { Ok(()) }.boxed());
        second_reliable_sender_handle
            .expect_send()
            .return_once(|_| async { Ok(()) }.boxed());

        let reliable_senders_map = HashMap::from([
            ("b".to_string(), first_reliable_sender_handle),
            ("c".to_string(), second_reliable_sender_handle),
        ]);

        let (_sender, receiver) = mpsc::channel(32);
        let (msg_sender, msg_receiver) = oneshot::channel();
        let mut echo_bcast_actor = EchoBroadcastActor::start(receiver);

        let msg: Message = Message::BroadcastMessage(Broadcast::RoundOnePackage(
            RoundOnePackageMessage {
                sender_id: "localhost".to_string(),
                message: "round one package".to_string(),
            },
            Some(MessageId(1)),
        ));

        let result = echo_bcast_actor
            .send_message(msg.clone(), reliable_senders_map, msg_sender)
            .await;

        assert!(result.is_ok());
        assert_eq!(echo_bcast_actor.message_client_txs.len(), 1);

        let echo_b: Message = Message::BroadcastMessage(Broadcast::RoundOnePackage(
            RoundOnePackageMessage {
                sender_id: "b".to_string(),
                message: "round one package".to_string(),
            },
            Some(MessageId(1)),
        ));
        let echo_c: Message = Message::BroadcastMessage(Broadcast::RoundOnePackage(
            RoundOnePackageMessage {
                sender_id: "c".to_string(),
                message: "round one package".to_string(),
            },
            Some(MessageId(1)),
        ));

        echo_bcast_actor.handle_received_echo(echo_b).await;

        echo_bcast_actor.handle_received_echo(echo_c).await;

        assert!(msg_receiver.await.is_ok());
    }
}