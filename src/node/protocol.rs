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

extern crate flexbuffers;
extern crate serde;
use serde::{Deserialize, Serialize};

mod handshake;
mod heartbeat;
mod ping;

pub use handshake::HandshakeMessage;
pub use heartbeat::HeartbeatMessage;
pub use ping::PingMessage;

#[mockall_double::double]
use super::reliable_sender::ReliableSenderHandle;

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub enum Message {
    Handshake(HandshakeMessage),
    Heartbeat(HeartbeatMessage),
    Ping(PingMessage),
}

/// Methods for all protocol messages
impl Message {
    /// Generates the response to send for a message received
    pub fn response_for_received(&self, node_id: &str) -> Result<Option<Message>, String> {
        match self {
            Message::Handshake(m) => m.response_for_received(node_id),
            Message::Heartbeat(m) => m.response_for_received(node_id),
            Message::Ping(m) => m.response_for_received(node_id),
        }
    }

    /// return the sender_id for all message types
    pub fn get_sender_id(&self) -> String {
        match self {
            Message::Handshake(m) => m.sender_id.clone(),
            Message::Heartbeat(m) => m.sender_id.clone(),
            Message::Ping(m) => m.sender_id.clone(),
        }
    }
}

pub async fn start_protocol<M>(handle: ReliableSenderHandle, node_id: &str)
where
    M: ProtocolMessage,
{
    if let Some(message) = M::start(node_id) {
        log::debug!("Sending initial handshake message");
        if let Err(e) = handle.send(message).await {
            log::info!("Error sending start protocol message {}", e);
        }
    }
}

/// Trait implemented by all protocol messages
pub trait ProtocolMessage
where
    Self: Sized,
{
    fn start(node_id: &str) -> Option<Message>;
    fn response_for_received(&self, node_id: &str) -> Result<Option<Message>, String>;
}

#[cfg(test)]
mod tests {

    use super::start_protocol;
    use super::HandshakeMessage;
    use super::Message;
    use super::PingMessage;
    use super::ProtocolMessage;
    use crate::node::reliable_sender::MockReliableSenderHandle;

    #[test]
    fn it_matches_start_message_for_ping() {
        let start_message = PingMessage::start("localhost").unwrap();
        assert_eq!(
            start_message,
            Message::Ping(PingMessage {
                sender_id: "localhost".to_string(),
                message: String::from("ping")
            })
        );
    }

    #[test]
    fn it_invokes_received_message_after_deseralization() {
        let msg = Message::Ping(PingMessage {
            sender_id: "localhost".to_string(),
            message: String::from("ping"),
        });

        let response = msg.response_for_received("localhost").unwrap();
        assert_eq!(
            response,
            Some(Message::Ping(PingMessage {
                sender_id: "localhost".to_string(),
                message: String::from("pong")
            }))
        );
    }

    #[tokio::test]
    async fn it_should_send_first_message_on_start_protocol() {
        let mut handle_mock = MockReliableSenderHandle::default();

        handle_mock.expect_send().return_once(|_| Ok(()));
        start_protocol::<HandshakeMessage>(handle_mock, "localhost".into()).await;
    }

    #[tokio::test]
    async fn it_should_quitely_move_on_if_error_on_start_protocol() {
        let mut handle_mock = MockReliableSenderHandle::default();

        handle_mock
            .expect_send()
            .return_once(|_| Err("Some error".into()));
        start_protocol::<HandshakeMessage>(handle_mock, "localhost".into()).await;
    }
}
