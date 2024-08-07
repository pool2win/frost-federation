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

use crate::node::protocol::Message;
#[mockall_double::double]
use crate::node::reliable_sender::ReliableSenderHandle;
use std::collections::HashMap;
use std::error::Error;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};
use tokio::time::timeout;

use super::connection::ConnectionResultSender;

/// Message types for echo broadcast actor -> handle communication
pub(crate) enum EchoBroadcastMessage {
    Send {
        data: Message,
        respond_to: ConnectionResultSender,
    },
    Echo {
        data: Message,
        respond_to: ConnectionResultSender,
    },
}

/// Echo Broadcast Actor model implementation.
/// The actor receives messages from EchoBroadcastHandle
pub(crate) struct EchoBroadcastActor {
    reliable_sender: ReliableSenderHandle,
    receiver: mpsc::Receiver<EchoBroadcastMessage>,
    echos: HashMap<String, bool>,
}

impl EchoBroadcastActor {
    pub fn start(
        reliable_sender: ReliableSenderHandle,
        receiver: mpsc::Receiver<EchoBroadcastMessage>,
        members: Vec<String>,
    ) -> Self {
        let mut echos = HashMap::<String, bool>::new();
        for member in members {
            echos.insert(member, false);
        }
        Self {
            reliable_sender,
            receiver,
            echos,
        }
    }

    pub async fn handle_message(&mut self, message: EchoBroadcastMessage) {
        match message {
            EchoBroadcastMessage::Send { data, respond_to } => {
                // TODO: Start book keeping for waiting for echo
                // Send message once book keeping has started
                let _ = self.send_message(data, respond_to).await;
            }
            EchoBroadcastMessage::Echo { data, respond_to } => {
                // TODO: Update waiting for echo
                self.handle_received_echo(data, respond_to).await;
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
        respond_to: ConnectionResultSender,
    ) -> Result<(), Box<dyn Error + Sync + Send>> {
        let sender_id = data.get_sender_id();
        if self.reliable_sender.send(data).await.is_err() {
            return Err("Error sending message using reliable sender".into());
        }
        self.echos.insert(sender_id, true);
        Ok(())
    }

    pub async fn handle_received_echo(
        &mut self,
        data: Message,
        respond_to: ConnectionResultSender,
    ) {
    }
}

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
}

async fn start_echo_broadcast(mut actor: EchoBroadcastActor) {
    while let Some(message) = actor.receiver.recv().await {
        actor.handle_message(message).await;
    }
}

/// Handle for the echo broadcast actor
impl EchoBroadcastHandle {
    pub fn start(
        reliable_sender: ReliableSenderHandle,
        delivery_timeout: u64,
        members: Vec<String>,
    ) -> Self {
        let (sender, receiver) = mpsc::channel(32);
        let actor = EchoBroadcastActor::start(reliable_sender, receiver, members);
        tokio::spawn(start_echo_broadcast(actor));

        Self {
            sender,
            delivery_timeout,
        }
    }

    pub async fn send_broadcast(&self, message: Message) -> Result<(), Box<dyn Error>> {
        let (sender_from_actor, receiver_from_actor) = oneshot::channel();
        let msg = EchoBroadcastMessage::Send {
            data: message,
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

#[cfg(test)]
mod tests {
    use crate::node::protocol::{HeartbeatMessage, ProtocolMessage};

    use super::*;

    #[tokio::test]
    async fn it_should_return_ok_on_send_broadcast() {
        let members = vec!["a".to_string(), "b".to_string()];
        let delivery_timeout = 10;
        let mock_reliable_sender = ReliableSenderHandle::default();
        let handle = EchoBroadcastHandle::start(mock_reliable_sender, delivery_timeout, members);

        let message = HeartbeatMessage::start("localhost".into()).unwrap();
        let result = handle.send_broadcast(message).await;
        assert!(result.is_ok());
    }
}

#[cfg(test)]
mod echo_broadcast_actor_tests {
    use crate::node::protocol::{HeartbeatMessage, PingMessage, ProtocolMessage};

    use super::*;

    #[tokio::test]
    async fn it_should_create_actor_with_echos_setup() {
        let members = vec!["a".to_string(), "b".to_string()];
        let (_sender, receiver) = mpsc::channel(32);
        let mock_reliable_sender = ReliableSenderHandle::default();
        let actor = EchoBroadcastActor::start(mock_reliable_sender, receiver, members);

        assert_eq!(actor.echos.keys().count(), 2);
    }

    #[tokio::test]
    async fn it_should_handle_send_broadcast_message_with_ok_from_reliable_sender() {
        let members = vec!["a".to_string(), "b".to_string()];
        let (_sender, receiver) = mpsc::channel(32);
        let (respond_to, waiting_for_response) = oneshot::channel();

        let mut mock_reliable_sender = ReliableSenderHandle::default();
        let _ = mock_reliable_sender.expect_send().return_once(|_| Ok(()));

        let mut actor = EchoBroadcastActor::start(mock_reliable_sender, receiver, members);

        let data = PingMessage::start("a").unwrap();
        let result = actor.send_message(data, respond_to).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn it_should_handle_send_broadcast_message_with_error_from_reliable_sender() {
        let members = vec!["a".to_string(), "b".to_string()];
        let (_sender, receiver) = mpsc::channel(32);
        let (respond_to, waiting_for_response) = oneshot::channel();

        let mut mock_reliable_sender = ReliableSenderHandle::default();
        let _ = mock_reliable_sender
            .expect_send()
            .return_once(|_| Err("some error".into()));

        let mut actor = EchoBroadcastActor::start(mock_reliable_sender, receiver, members);

        let data = PingMessage::start("a").unwrap();
        let result = actor.send_message(data, respond_to).await;
        assert!(result.is_err());
    }
}
