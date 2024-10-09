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
mod round_one_package;

use crate::node::state::State;
use futures::{Future, FutureExt};
pub use handshake::{Handshake, HandshakeMessage};
pub use heartbeat::{Heartbeat, HeartbeatMessage};
pub use membership::{Membership, MembershipMessage};
pub use ping::{Ping, PingMessage};
pub use round_one_package::{RoundOnePackage, RoundOnePackageMessage};
use serde::{Deserialize, Serialize};
use std::pin::Pin;
use std::task::{Context, Poll};
use tower::{util::BoxService, BoxError, Service, ServiceExt};

use self::message_id_generator::MessageId;

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
#[serde(untagged)]
pub enum Message {
    UnicastMessage(Unicast),
    BroadcastMessage(Broadcast),
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub enum Unicast {
    Handshake(HandshakeMessage),
    Heartbeat(HeartbeatMessage),
    Ping(PingMessage),
    Membership(MembershipMessage),
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub enum Broadcast {
    RoundOnePackage(RoundOnePackageMessage, Option<MessageId>),
}

pub trait NetworkMessage {
    fn get_sender_id(&self) -> String;
    fn get_message_id(&self) -> Option<MessageId>;
}

/// Methods for all protocol messages
impl NetworkMessage for Message {
    /// return the sender_id for all message types
    fn get_sender_id(&self) -> String {
        match self {
            Message::UnicastMessage(m) => match m {
                Unicast::Handshake(m) => m.sender_id.clone(),
                Unicast::Heartbeat(m) => m.sender_id.clone(),
                Unicast::Ping(m) => m.sender_id.clone(),
                Unicast::Membership(m) => m.sender_id.clone(),
            },
            Message::BroadcastMessage(m) => match m {
                Broadcast::RoundOnePackage(m, _) => m.sender_id.clone(),
            },
        }
    }

    /// Return the message_id for all message types
    /// For now we return the MessageId for Broadcasts. For unicasts, we return None.
    fn get_message_id(&self) -> Option<MessageId> {
        match self {
            Message::UnicastMessage(_m) => None,
            Message::BroadcastMessage(m) => match m {
                Broadcast::RoundOnePackage(_m, mid) => mid.clone(),
            },
        }
    }
}

impl From<HeartbeatMessage> for Message {
    fn from(value: HeartbeatMessage) -> Self {
        Message::UnicastMessage(Unicast::Heartbeat(value))
    }
}

impl From<HandshakeMessage> for Message {
    fn from(value: HandshakeMessage) -> Self {
        Message::UnicastMessage(Unicast::Handshake(value))
    }
}

impl From<PingMessage> for Message {
    fn from(value: PingMessage) -> Self {
        Message::UnicastMessage(Unicast::Ping(value))
    }
}

impl From<MembershipMessage> for Message {
    fn from(value: MembershipMessage) -> Self {
        Message::UnicastMessage(Unicast::Membership(value))
    }
}

impl From<RoundOnePackageMessage> for Message {
    fn from(value: RoundOnePackageMessage) -> Self {
        Message::BroadcastMessage(Broadcast::RoundOnePackage(value, None))
    }
}

impl From<Broadcast> for Message {
    fn from(value: Broadcast) -> Self {
        match value {
            Broadcast::RoundOnePackage(m, message_id) => {
                Message::BroadcastMessage(Broadcast::RoundOnePackage(m, message_id))
            }
        }
    }
}

#[derive(Clone)]
pub struct Protocol {
    node_id: String,
    state: State,
}

impl Protocol {
    pub fn new(node_id: String, state: State) -> Self {
        Protocol { node_id, state }
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
        let state = self.state.clone();
        async move {
            let svc = match &msg {
                Message::UnicastMessage(Unicast::Ping(_m)) => BoxService::new(Ping::new(sender_id)),
                Message::UnicastMessage(Unicast::Handshake(_m)) => {
                    BoxService::new(Handshake::new(sender_id))
                }
                Message::UnicastMessage(Unicast::Heartbeat(_m)) => {
                    BoxService::new(Heartbeat::new(sender_id))
                }
                Message::UnicastMessage(Unicast::Membership(_m)) => {
                    BoxService::new(Membership::new(sender_id))
                }
                Message::BroadcastMessage(Broadcast::RoundOnePackage(_m, _)) => {
                    BoxService::new(RoundOnePackage::new(sender_id, state))
                }
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
    use crate::node::state::State;
    use crate::node::MembershipHandle;
    use crate::node::MessageIdGenerator;

    #[tokio::test]
    async fn it_should_create_protocol() {
        let message_id_generator = MessageIdGenerator::new("localhost".to_string());
        let membership_handle = MembershipHandle::start("localhost".to_string()).await;
        let state = State::new(membership_handle, message_id_generator);

        let p = Protocol::new("local".into(), state);
        let m = p.oneshot(PingMessage::default().into()).await;
        assert!(m.unwrap().is_some());
    }
}
