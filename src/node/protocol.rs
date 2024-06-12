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

use std::error::Error;
use tokio_util::bytes::Bytes;
extern crate flexbuffers;
extern crate serde;
use serde::{Deserialize, Serialize};

mod handshake;
mod heartbeat;
mod ping;

pub use handshake::HandshakeMessage;
pub use heartbeat::HeartbeatMessage;
pub use ping::PingMessage;

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
    pub fn response_for_received(&self) -> Result<Option<Message>, String> {
        match self {
            Message::Handshake(m) => m.response_for_received(),
            Message::Heartbeat(m) => m.response_for_received(),
            Message::Ping(m) => m.response_for_received(),
        }
    }
}

pub async fn start_protocol<M>(handle: ReliableSenderHandle, init: bool)
where
    M: ProtocolMessage,
{
    if init {
        if let Some(message) = M::start() {
            log::debug!("Sending initial handshake message");
            if let Err(e) = handle.send(message).await {
                log::info!("Error sending start protocol message {}", e);
            }
        }
    }
}

/// Trait implemented by all protocol messages
pub trait ProtocolMessage
where
    Self: Sized,
{
    fn start() -> Option<Message>;
    fn response_for_received(&self) -> Result<Option<Message>, String>;
}

#[cfg(test)]
mod tests {
    use super::Message;
    use super::PingMessage;
    use super::ProtocolMessage;

    #[test]
    fn it_matches_start_message_for_ping() {
        let start_message = PingMessage::start().unwrap();
        assert_eq!(
            start_message,
            Message::Ping(PingMessage {
                message: String::from("ping")
            })
        );
    }

    #[test]
    fn it_invokes_received_message_after_deseralization() {
        let msg = Message::Ping(PingMessage {
            message: String::from("ping"),
        });

        let response = msg.response_for_received().unwrap();
        assert_eq!(
            response,
            Some(Message::Ping(PingMessage {
                message: String::from("pong")
            }))
        );
    }
}
