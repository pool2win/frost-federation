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

use crate::node::protocol::{MembershipMessage, Message, Unicast};
#[mockall_double::double]
use crate::node::reliable_sender::ReliableSenderHandle;
use crate::node::state::State;
use futures::{Future, FutureExt};
use serde::{Deserialize, Serialize};
use std::pin::Pin;
use std::task::{Context, Poll};
use tower::{BoxError, Service};

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Default)]
pub struct HandshakeMessage {
    pub sender_id: String,
    pub message: String,
    pub version: String,
}

impl HandshakeMessage {
    pub fn new(sender_id: String, message: String, version: String) -> Self {
        Self {
            sender_id,
            message,
            version,
        }
    }
}

#[derive(Clone)]
pub struct Handshake {
    node_id: String,
    state: State,
    peer_sender: ReliableSenderHandle,
}

impl Handshake {
    pub fn new(node_id: String, state: State, peer_sender: ReliableSenderHandle) -> Self {
        Handshake {
            node_id,
            state,
            peer_sender,
        }
    }
}

/// Service for handling Handshake protocol.
///
/// By making all protocol into a Service, we can use tower:Steer to
/// multiplex across services.
impl Service<Message> for Handshake {
    type Response = Option<Message>;
    type Error = BoxError;
    type Future = Pin<Box<dyn Future<Output = Result<Option<Message>, BoxError>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, msg: Message) -> Self::Future {
        let local_sender_id = self.node_id.clone();
        let membership_handle = self.state.membership_handle.clone();
        let peer_sender = self.peer_sender.clone();
        async move {
            match msg {
                Message::Unicast(Unicast::Handshake(HandshakeMessage {
                    message,
                    sender_id,
                    version,
                })) => match message.as_str() {
                    "helo" => {
                        if membership_handle
                            .add_member(sender_id, peer_sender)
                            .await
                            .is_err()
                        {
                            return Err("Error adding new member".into());
                        }
                        Ok(Some(Message::Unicast(Unicast::Handshake(
                            HandshakeMessage {
                                message: "oleh".to_string(),
                                sender_id: local_sender_id,
                                version: "0.1.0".to_string(),
                            },
                        ))))
                    }
                    "oleh" => {
                        if membership_handle
                            .add_member(sender_id, peer_sender)
                            .await
                            .is_err()
                        {
                            return Err("Error adding new member".into());
                        }
                        let new_membership = membership_handle
                            .get_members()
                            .await
                            .unwrap()
                            .into_keys()
                            .collect();
                        Ok(Some(Message::Unicast(Unicast::Membership(
                            MembershipMessage::new(local_sender_id, Some(new_membership)),
                        ))))
                    }
                    _ => Ok(Some(Message::Unicast(Unicast::Handshake(
                        HandshakeMessage {
                            message: "helo".to_string(),
                            sender_id: local_sender_id,
                            version: "0.1.0".to_string(),
                        },
                    )))),
                },
                _ => Ok(None),
            }
        }
        .boxed()
    }
}

#[cfg(test)]
mod handshake_tests {
    use crate::node::membership::MembershipHandle;
    use crate::node::protocol::{
        message_id_generator::MessageIdGenerator, Handshake, HandshakeMessage, MembershipMessage,
        Message, Unicast,
    };
    #[mockall_double::double]
    use crate::node::reliable_sender::ReliableSenderHandle;
    use crate::node::state::State;
    use tower::{Service, ServiceExt};

    #[tokio::test]
    async fn it_should_create_handshake_as_service_and_respond_to_default_message_with_handshake() {
        let membership_handle = MembershipHandle::start("local".into()).await;
        let state = State::new(membership_handle, MessageIdGenerator::new("local".into()));
        let mut reliable_sender_handle = ReliableSenderHandle::default();
        reliable_sender_handle
            .expect_clone()
            .returning(ReliableSenderHandle::default);

        let mut p = Handshake::new("local".to_string(), state, reliable_sender_handle);

        let res = p
            .ready()
            .await
            .unwrap()
            .call(HandshakeMessage::default().into())
            .await
            .unwrap();
        assert!(res.is_some());
        assert_eq!(
            res,
            Some(Message::Unicast(super::Unicast::Handshake(
                HandshakeMessage {
                    message: "helo".to_string(),
                    sender_id: "local".to_string(),
                    version: "0.1.0".to_string()
                }
            )))
        );
    }

    #[tokio::test]
    async fn it_should_create_handshake_as_service_and_respond_to_helo_with_oleh() {
        let membership_handle = MembershipHandle::start("local".into()).await;
        let state = State::new(membership_handle, MessageIdGenerator::new("local".into()));
        let mut reliable_sender_handle = ReliableSenderHandle::default();
        reliable_sender_handle
            .expect_clone()
            .returning(ReliableSenderHandle::default);

        let mut p = Handshake::new("local".to_string(), state, reliable_sender_handle);

        let res = p
            .ready()
            .await
            .unwrap()
            .call(Message::Unicast(super::Unicast::Handshake(
                HandshakeMessage {
                    message: "helo".to_string(),
                    sender_id: "local".to_string(),
                    version: "0.1.0".to_string(),
                },
            )))
            .await
            .unwrap();
        assert!(res.is_some());
        assert_eq!(
            res,
            Some(Message::Unicast(super::Unicast::Handshake(
                HandshakeMessage {
                    message: "oleh".to_string(),
                    sender_id: "local".to_string(),
                    version: "0.1.0".to_string()
                }
            )))
        );
    }

    #[tokio::test]
    async fn it_should_create_handshake_as_service_and_respond_to_oleh_with_none() {
        let membership_handle = MembershipHandle::start("local".into()).await;
        let state = State::new(membership_handle, MessageIdGenerator::new("local".into()));
        let mut reliable_sender_handle = ReliableSenderHandle::default();
        reliable_sender_handle.expect_clone().returning(|| {
            let mut mock = ReliableSenderHandle::default();
            mock.expect_clone().returning(ReliableSenderHandle::default);
            mock
        });

        let mut p = Handshake::new("local".to_string(), state, reliable_sender_handle);

        let res = p
            .ready()
            .await
            .unwrap()
            .call(Message::Unicast(super::Unicast::Handshake(
                HandshakeMessage {
                    message: "oleh".to_string(),
                    sender_id: "local".to_string(),
                    version: "0.1.0".to_string(),
                },
            )))
            .await
            .unwrap();
        assert!(res.is_some());
        assert_eq!(
            res,
            Some(Message::Unicast(Unicast::Membership(
                MembershipMessage::new("local".into(), Some(vec!["local".to_string()])),
            )))
        );
    }
}
