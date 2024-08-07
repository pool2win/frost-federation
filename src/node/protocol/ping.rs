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

use super::{Message, ProtocolMessage};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct PingMessage {
    pub sender_id: String,
    pub message: String,
}

impl ProtocolMessage for PingMessage {
    fn start(node_id: &str) -> Option<Message> {
        Some(Message::Ping(PingMessage {
            sender_id: node_id.to_string(),
            message: String::from("ping"),
        }))
    }

    fn response_for_received(&self, id: &str) -> Result<Option<Message>, String> {
        if self.message == "ping" {
            Ok(Some(Message::Ping(PingMessage {
                sender_id: id.to_string(),
                message: String::from("pong"),
            })))
        } else {
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {

    use crate::node::protocol::{Message, PingMessage, ProtocolMessage};

    #[test]
    fn it_matches_start_message_for_ping() {
        if let Some(Message::Ping(start_message)) = PingMessage::start("localhost".into()) {
            assert_eq!(start_message.message, "ping".to_string());
        }
    }

    #[test]
    fn it_matches_response_message_for_correct_handshake_start() {
        let start_message = PingMessage::start("localhost".into()).unwrap();
        let response = start_message
            .response_for_received("localhost")
            .unwrap()
            .unwrap();
        assert_eq!(
            response,
            Message::Ping(PingMessage {
                sender_id: "localhost".to_string(),
                message: "pong".to_string()
            })
        );
    }
}
