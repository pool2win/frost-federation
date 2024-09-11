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
use super::protocol::message_id_generator::{self, MessageId};
use crate::node::protocol::Message;
#[mockall_double::double]
use crate::node::reliable_sender::ReliableSenderHandle;
use std::collections::HashMap;
use std::error::Error;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};
use tokio::time::timeout;

type EchosMap = HashMap<MessageId, HashMap<String, bool>>;

/// Message types for echo broadcast actor -> handle communication
pub(crate) enum EchoBroadcastMessage {
    Send {
        data: Message,
        message_id: MessageId,
        respond_to: ConnectionResultSender,
    },
    Echo {
        data: Message, // TODO: this can be taken away, so only the message_id acts as an ack/echo
        message_id: MessageId,
    },
}

/// Echo Broadcast Actor model implementation.
/// The actor receives messages from EchoBroadcastHandle
pub(crate) struct EchoBroadcastActor {
    reliable_sender_map: ReliableSenderMap,
    receiver: mpsc::Receiver<EchoBroadcastMessage>,
    echos: EchosMap,
    responders: HashMap<MessageId, ConnectionResultSender>,
}

impl EchoBroadcastActor {
    pub fn start(
        receiver: mpsc::Receiver<EchoBroadcastMessage>,
        reliable_sender_map: ReliableSenderMap,
    ) -> Self {
        Self {
            reliable_sender_map,
            receiver,
            echos: EchosMap::new(),
            responders: HashMap::new(),
        }
    }

    pub async fn handle_message(&mut self, message: EchoBroadcastMessage) {
        match message {
            EchoBroadcastMessage::Send {
                data,
                message_id,
                respond_to,
            } => {
                let _ = self.send_message(data, message_id, respond_to).await;
            }
            EchoBroadcastMessage::Echo { data, message_id } => {
                self.handle_received_echo(data, message_id).await;
            }
        }
    }

    /// Receives messages to echo broadcast, wait for echo messages
    /// from all members, on timeout, return an error to waiting
    /// receiver.
    ///
    /// Process:
    /// 1. Wait for echos from all members for the message sent
    /// 2. On timeout return error
    /// 3. On receiving echos from all members, return Ok() to waiting receiver
    pub async fn send_message(
        &mut self,
        data: Message,
        message_id: MessageId,
        respond_to: ConnectionResultSender,
    ) -> Result<(), Box<dyn Error + Sync + Send>> {
        let sender_id = data.get_sender_id();
        for (member, reliable_sender) in self.reliable_sender_map.iter_mut() {
            if reliable_sender.send(data.clone()).await.is_err() {
                return Err("Error sending message using reliable sender".into());
            }
            let details = self
                .echos
                .entry(message_id.clone())
                .or_insert(HashMap::new());
            details.entry(member.clone()).or_insert(false);
        }
        // Update sender_id's echo to true
        self.echos
            .get_mut(&message_id)
            .unwrap()
            .insert(sender_id, true);

        self.responders.insert(message_id, respond_to);
        Ok(())
    }

    /// Handle Echo messages received for this message
    pub async fn handle_received_echo(&mut self, data: Message, message_id: MessageId) {
        let sender_id = data.get_sender_id();
        self.echos
            .get_mut(&message_id)
            .unwrap()
            .insert(sender_id, true);

        if self
            .echos
            .get(&message_id)
            .unwrap()
            .iter()
            .all(|(_sender_id, status)| *status)
        {
            match self.responders.remove(&message_id) {
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

use super::protocol::message_id_generator::MessageIdGenerator;

/// Handler for sending and confirming echo messages for a broadcast
/// Send using ReliableSenderHandle and then wait fro Echo message
///
/// Members list is copied into this struct so that we are only
/// waiting for echos from the parties that were members when the
/// broadcast was originally sent.
#[derive(Clone, Debug)]
pub(crate) struct EchoBroadcastHandle {
    sender: mpsc::Sender<EchoBroadcastMessage>,
    delivery_timeout: u64,
    message_id_generator: MessageIdGenerator,
}

async fn start_echo_broadcast(mut actor: EchoBroadcastActor) {
    while let Some(message) = actor.receiver.recv().await {
        actor.handle_message(message).await;
    }
}

/// Handle for the echo broadcast actor
impl EchoBroadcastHandle {
    pub fn start(
        message_id_generator: MessageIdGenerator,
        delivery_timeout: u64,
        reliable_senders_map: ReliableSenderMap,
    ) -> Self {
        let (sender, receiver) = mpsc::channel(32);
        let actor = EchoBroadcastActor::start(receiver, reliable_senders_map);
        tokio::spawn(start_echo_broadcast(actor));

        Self {
            sender,
            delivery_timeout,
            message_id_generator,
        }
    }

    /// Keep the same signature to send, so we can convert that into a Trait later if we want.
    pub async fn send(&self, message: Message) -> ConnectionResult<()> {
        let (sender_from_actor, receiver_from_actor) = oneshot::channel();
        let msg = EchoBroadcastMessage::Send {
            data: message,
            message_id: self.message_id_generator.next(),
            respond_to: sender_from_actor,
        };
        self.sender.send(msg).await.unwrap();
        if timeout(
            Duration::from_millis(self.delivery_timeout),
            receiver_from_actor,
        )
        .await
        .is_err()
        {
            Err("Broadcast timed out".into())
        } else {
            Ok(())
        }
    }
}

mockall::mock! {
    #[derive(Debug)]
    pub EchoBroadcastHandle {
        pub fn start(delivery_timeout: u64, reliable_senders_map:ReliableSenderMap) -> Self;
        pub async fn send(&self, message: Message) -> Result<(), Box<dyn Error>>;
    }
    impl Clone for EchoBroadcastHandle {
        fn clone(&self) -> Self;
    }
}

#[cfg(test)]
mod tests {
    use crate::node::protocol::{HeartbeatMessage, ProtocolMessage};

    use super::*;

    #[tokio::test]
    async fn it_should_return_ok_on_send() {
        let delivery_timeout = 10;
        let mock_reliable_sender = ReliableSenderHandle::default();
        let reliable_senders_map = HashMap::from([("a".to_string(), mock_reliable_sender)]);
        let message_id_generator = MessageIdGenerator::new("localhost".to_string());

        let handle = EchoBroadcastHandle::start(
            message_id_generator,
            delivery_timeout,
            reliable_senders_map,
        );

        let message = HeartbeatMessage::start("localhost".into()).unwrap();
        let result = handle.send(message).await;
        assert!(result.is_ok());
    }
}

#[cfg(test)]
mod echo_broadcast_actor_tests {
    use crate::node::protocol::{PingMessage, ProtocolMessage};

    use super::*;

    #[tokio::test]
    async fn it_should_create_actor_with_echos_setup() {
        let (_sender, receiver) = mpsc::channel(32);
        let mock_reliable_sender = ReliableSenderHandle::default();
        let reliable_senders_map = HashMap::from([("a".to_string(), mock_reliable_sender)]);
        let actor = EchoBroadcastActor::start(receiver, reliable_senders_map);

        assert_eq!(actor.echos.keys().count(), 0);
    }

    #[tokio::test]
    async fn it_should_handle_send_message_with_ok_from_reliable_sender() {
        let (_sender, receiver) = mpsc::channel(32);
        let (respond_to, waiting_for_response) = oneshot::channel();

        let mut mock_reliable_sender = ReliableSenderHandle::default();
        let _ = mock_reliable_sender.expect_send().return_once(|_| Ok(()));

        let reliable_senders_map = HashMap::from([("a".to_string(), mock_reliable_sender)]);

        let mut actor = EchoBroadcastActor::start(receiver, reliable_senders_map);

        let data = PingMessage::start("a").unwrap();
        let result = actor.send_message(data, MessageId(0), respond_to).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn it_should_handle_send_message_with_error_from_reliable_sender() {
        let (_sender, receiver) = mpsc::channel(32);
        let (respond_to, waiting_for_response) = oneshot::channel();

        let mut mock_reliable_sender = ReliableSenderHandle::default();
        let _ = mock_reliable_sender
            .expect_send()
            .return_once(|_| Err("some error".into()));

        let reliable_senders_map = HashMap::from([("a".to_string(), mock_reliable_sender)]);

        let mut actor = EchoBroadcastActor::start(receiver, reliable_senders_map);

        let data = PingMessage::start("a").unwrap();
        let result = actor.send_message(data, MessageId(0), respond_to).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn it_should_send_without_errors_if_no_errors_returned_from_echo_broadcasts() {
        let mut first_reliable_sender_handle = ReliableSenderHandle::default();
        let mut second_reliable_sender_handle = ReliableSenderHandle::default();
        let msg = PingMessage::start("a").unwrap();

        first_reliable_sender_handle
            .expect_clone()
            .returning(ReliableSenderHandle::default);
        second_reliable_sender_handle
            .expect_clone()
            .returning(ReliableSenderHandle::default);

        let mut mock_echo_bcast = MockEchoBroadcastHandle::default();
        mock_echo_bcast.expect_send().return_once(|_| Ok(()));

        let result = mock_echo_bcast.send(msg).await;
        assert!(result.is_ok());
    }
}
