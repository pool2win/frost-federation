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

mod handshake;
mod heartbeat;
pub(crate) mod message_id_generator;
mod ping;

#[mockall_double::double]
use super::reliable_sender::ReliableSenderHandle;
use futures::Future;
pub use handshake::{Handshake, HandshakeMessage};
pub use heartbeat::{Heartbeat, HeartbeatMessage};
pub use ping::{Ping, PingMessage};
use serde::{Deserialize, Serialize};
use std::pin::Pin;
use tower::{util::BoxService, BoxError, Service};

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub enum Message {
    Handshake(HandshakeMessage),
    Heartbeat(HeartbeatMessage),
    Ping(PingMessage),
}

/// Methods for all protocol messages
impl Message {
    /// return the sender_id for all message types
    pub fn get_sender_id(&self) -> String {
        match self {
            Message::Handshake(m) => m.sender_id.clone(),
            Message::Heartbeat(m) => m.sender_id.clone(),
            Message::Ping(m) => m.sender_id.clone(),
        }
    }
}

/// Build a service to use based on the message's type
pub fn service_for(
    message: &Message,
    node_id: String,
) -> BoxService<Option<Message>, Option<Message>, BoxError> {
    match message {
        Message::Ping(_m) => BoxService::new(Ping::new(node_id)),
        Message::Handshake(_m) => BoxService::new(Handshake::new(node_id)),
        Message::Heartbeat(_m) => BoxService::new(Heartbeat::new(node_id)),
    }
}

#[cfg(test)]
mod protocol_tests {}
