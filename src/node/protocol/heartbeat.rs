// Copyright 2024 Braidpool Developers

// This file is part of Braidpool

// Braidpool is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Braidpool is distributed in the hope that it will be useful, but
// WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU
// General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Braidpool. If not, see <https://www.gnu.org/licenses/>.

use super::{Message, ProtocolMessage};
use serde::{Deserialize, Serialize};
use std::time::SystemTime;

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct HeartbeatMessage {
    pub sender_id: String,
    pub time: SystemTime,
}

impl ProtocolMessage for HeartbeatMessage {
    fn start(node_id: &str) -> Option<Message> {
        Some(Message::Heartbeat(HeartbeatMessage {
            sender_id: node_id.to_string(),
            time: SystemTime::now(),
        }))
    }

    fn response_for_received(&self, _id: &str) -> Result<Option<Message>, String> {
        log::info!("Received {:?}", self);
        Ok(None)
    }
}

#[cfg(test)]
mod tests {

    use std::time::SystemTime;

    use crate::node::protocol::{HeartbeatMessage, Message, ProtocolMessage};

    #[test]
    fn it_matches_start_message_for_handshake() {
        if let Some(Message::Heartbeat(start_message)) = HeartbeatMessage::start("localhost".into())
        {
            assert!(start_message.time < SystemTime::now());
        }
    }

    #[test]
    fn it_matches_response_message_for_correct_handshake_start() {
        let start_message = HeartbeatMessage::start("localhost".into()).unwrap();
        let response = start_message.response_for_received("localhost").unwrap();
        assert_eq!(response, None);
    }
}
