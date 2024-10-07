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
mod membership;
pub(crate) mod message_id_generator;
mod ping;

use futures::{Future, FutureExt};
pub use handshake::{Handshake, HandshakeMessage};
pub use heartbeat::{Heartbeat, HeartbeatMessage};
pub use membership::{Membership, MembershipMessage};
pub use ping::{Ping, PingMessage};
use serde::{Deserialize, Serialize};
use std::pin::Pin;
use std::task::{Context, Poll};
use tower::{util::BoxService, BoxError, Service, ServiceExt};

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub enum Message {
    Handshake(HandshakeMessage),
    Heartbeat(HeartbeatMessage),
    Ping(PingMessage),
    Membership(MembershipMessage),
    BroadcastPing(PingMessage),
    EchoPing(PingMessage),
}

pub trait NetworkMessage {
    fn get_sender_id(&self) -> String;
}

/// Methods for all protocol messages
impl NetworkMessage for Message {
    /// return the sender_id for all message types
    fn get_sender_id(&self) -> String {
        match self {
            Message::Handshake(m) => m.sender_id.clone(),
            Message::Heartbeat(m) => m.sender_id.clone(),
            Message::Ping(m) => m.sender_id.clone(),
            Message::Membership(m) => m.sender_id.clone(),
            Message::BroadcastPing(m) => m.sender_id.clone(),
            Message::EchoPing(m) => m.sender_id.clone(),
        }
    }
}

impl From<HeartbeatMessage> for Message {
    fn from(value: HeartbeatMessage) -> Self {
        Message::Heartbeat(value)
    }
}

impl From<HandshakeMessage> for Message {
    fn from(value: HandshakeMessage) -> Self {
        Message::Handshake(value)
    }
}

impl From<PingMessage> for Message {
    fn from(value: PingMessage) -> Self {
        Message::Ping(value)
    }
}

impl From<MembershipMessage> for Message {
    fn from(value: MembershipMessage) -> Self {
        Message::Membership(value)
    }
}

#[derive(Debug, Clone)]
pub struct Protocol {
    node_id: String,
}

impl Protocol {
    pub fn new(node_id: String) -> Self {
        Protocol { node_id }
    }
}

impl Service<Message> for Protocol {
    type Response = Option<Message>;
    type Error = BoxError;
    type Future = Pin<Box<dyn Future<Output = Result<Option<Message>, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, msg: Message) -> Self::Future {
        let sender_id = self.node_id.clone();
        async move {
            let svc = match &msg {
                Message::Ping(_m) => BoxService::new(Ping::new(sender_id)),
                Message::Handshake(_m) => BoxService::new(Handshake::new(sender_id)),
                Message::Heartbeat(_m) => BoxService::new(Heartbeat::new(sender_id)),
                Message::Membership(_m) => BoxService::new(Membership::new(sender_id)),
                Message::BroadcastPing(_m) => BoxService::new(Ping::new(sender_id)),
                Message::EchoPing(_m) => BoxService::new(Ping::new(sender_id)),
            };
            svc.oneshot(msg).await
        }
        .boxed()
    }
}

#[cfg(test)]
mod protocol_tests {

    use tower::ServiceExt;

    use super::Protocol;
    use crate::node::protocol::ping::PingMessage;

    #[tokio::test]
    async fn it_should_create_protocol() {
        let p = Protocol::new("local".to_string());
        let m = p.oneshot(PingMessage::default().into()).await;
        assert!(m.unwrap().is_some());
    }
}
