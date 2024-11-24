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

pub(crate) mod dkg;
mod handshake;
mod heartbeat;
pub mod init;
mod membership;
pub(crate) mod message_id_generator;
mod ping;

use self::message_id_generator::MessageId;
#[mockall_double::double]
use super::reliable_sender::ReliableSenderHandle;
use crate::node::state::State;
pub use handshake::{Handshake, HandshakeMessage};
pub use heartbeat::{Heartbeat, HeartbeatMessage};
pub use membership::{Membership, MembershipMessage};
pub use ping::{Ping, PingMessage};

use futures::{Future, FutureExt};
use serde::{Deserialize, Serialize};
use std::pin::Pin;
use std::task::{Context, Poll};
use tower::{util::BoxService, BoxError, Service, ServiceExt};

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub enum Message {
    Unicast(Unicast),
    Broadcast(BroadcastProtocol, Option<MessageId>),
    Echo(BroadcastProtocol, MessageId, String),
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub enum Unicast {
    Handshake(HandshakeMessage),
    Heartbeat(HeartbeatMessage),
    Ping(PingMessage),
    Membership(MembershipMessage),
    DKGRoundTwoPackage(dkg::round_two::PackageMessage),
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub enum BroadcastProtocol {
    DKGRoundOnePackage(dkg::round_one::PackageMessage),
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
            Message::Unicast(m) => match m {
                Unicast::Handshake(m) => m.sender_id.clone(),
                Unicast::Heartbeat(m) => m.sender_id.clone(),
                Unicast::Ping(m) => m.sender_id.clone(),
                Unicast::Membership(m) => m.sender_id.clone(),
                Unicast::DKGRoundTwoPackage(m) => m.sender_id.clone(),
            },
            Message::Broadcast(m, _) => match m {
                BroadcastProtocol::DKGRoundOnePackage(m) => m.sender_id.clone(),
            },
            Message::Echo(_, _, peer_id) => peer_id.to_string(),
        }
    }

    /// return the message_id for all message types
    /// For now we return the MessageId for Broadcasts. For unicasts, we return None.
    fn get_message_id(&self) -> Option<MessageId> {
        match self {
            Message::Unicast(_m) => None,
            Message::Broadcast(m, mid) => match m {
                BroadcastProtocol::DKGRoundOnePackage(_m) => mid.clone(),
            },
            Message::Echo(m, mid, _) => match m {
                BroadcastProtocol::DKGRoundOnePackage(_m) => Some(mid.clone()),
            },
        }
    }
}

impl From<HeartbeatMessage> for Message {
    fn from(value: HeartbeatMessage) -> Self {
        Message::Unicast(Unicast::Heartbeat(value))
    }
}

impl From<HandshakeMessage> for Message {
    fn from(value: HandshakeMessage) -> Self {
        Message::Unicast(Unicast::Handshake(value))
    }
}

impl From<PingMessage> for Message {
    fn from(value: PingMessage) -> Self {
        Message::Unicast(Unicast::Ping(value))
    }
}

impl From<MembershipMessage> for Message {
    fn from(value: MembershipMessage) -> Self {
        Message::Unicast(Unicast::Membership(value))
    }
}

impl From<dkg::round_one::PackageMessage> for Message {
    fn from(value: dkg::round_one::PackageMessage) -> Self {
        Message::Broadcast(BroadcastProtocol::DKGRoundOnePackage(value), None)
    }
}

impl From<dkg::round_two::PackageMessage> for Message {
    fn from(value: dkg::round_two::PackageMessage) -> Self {
        Message::Unicast(Unicast::DKGRoundTwoPackage(value))
    }
}

impl From<BroadcastProtocol> for Message {
    fn from(value: BroadcastProtocol) -> Self {
        match value {
            BroadcastProtocol::DKGRoundOnePackage(m) => {
                Message::Broadcast(BroadcastProtocol::DKGRoundOnePackage(m), None)
            }
        }
    }
}

#[derive(Clone)]
pub struct Protocol {
    node_id: String,
    state: State,
    peer_sender: ReliableSenderHandle,
}

impl Protocol {
    pub fn new(node_id: String, state: State, peer_sender: ReliableSenderHandle) -> Self {
        Protocol {
            node_id,
            state,
            peer_sender,
        }
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
        let peer_sender = self.peer_sender.clone();
        async move {
            let svc = match &msg {
                Message::Unicast(Unicast::Ping(_m)) => BoxService::new(Ping::new(sender_id)),
                Message::Unicast(Unicast::Handshake(_m)) => {
                    BoxService::new(Handshake::new(sender_id, state, peer_sender))
                }
                Message::Unicast(Unicast::Heartbeat(_m)) => {
                    BoxService::new(Heartbeat::new(sender_id))
                }
                Message::Unicast(Unicast::Membership(_m)) => {
                    BoxService::new(Membership::new(sender_id, state))
                }
                Message::Broadcast(BroadcastProtocol::DKGRoundOnePackage(_m), _) => {
                    BoxService::new(dkg::round_one::Package::new(sender_id, state))
                }
                Message::Echo(_, _, _) => {
                    // Don't respond to echo messages, by returning None
                    BoxService::new(tower::service_fn(|_| async { Ok(None) }))
                }
                Message::Unicast(Unicast::DKGRoundTwoPackage(_m)) => {
                    BoxService::new(dkg::round_two::Package::new(sender_id, state))
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
    #[mockall_double::double]
    use crate::node::reliable_sender::ReliableSenderHandle;
    use crate::node::state::State;
    use crate::node::MembershipHandle;
    use crate::node::MessageIdGenerator;

    #[tokio::test]
    async fn it_should_create_protocol() {
        let mut reliable_sender_handle = ReliableSenderHandle::default();
        reliable_sender_handle
            .expect_clone()
            .returning(ReliableSenderHandle::default);
        let message_id_generator = MessageIdGenerator::new("localhost".to_string());
        let membership_handle = MembershipHandle::start("localhost".to_string()).await;
        let state = State::new(membership_handle, message_id_generator);

        let p = Protocol::new("local".into(), state, reliable_sender_handle);
        let m = p.oneshot(PingMessage::default().into()).await;
        assert!(m.unwrap().is_some());
    }
}
