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
pub struct NoiseHandshakeMessage {
    pub message: String,
}

impl ProtocolMessage for NoiseHandshakeMessage {
    fn start() -> Option<Message> {
        Some(Message::NoiseHandshake(NoiseHandshakeMessage {
            message: String::from("-> e"),
        }))
    }

    fn response_for_received(&self) -> Result<Option<Message>, String> {
        match self.message.as_str() {
            "-> e" => Ok(Some(Message::NoiseHandshake(NoiseHandshakeMessage {
                message: String::from("<- e, ee, s, es"),
            }))),
            "<- e, ee, s, es" => Ok(Some(Message::NoiseHandshake(NoiseHandshakeMessage {
                message: String::from("-> s, se"),
            }))),
            "-> s, se" => Ok(Some(Message::NoiseHandshake(NoiseHandshakeMessage {
                message: String::from("finished"),
            }))),
            _ => Ok(None),
        }
    }
}
