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

use crate::node::protocol::{Message, Unicast};
use crate::node::state::State;
use futures::{Future, FutureExt};
use serde::{Deserialize, Serialize};
use std::pin::Pin;
use std::task::{Context, Poll};
use tower::{BoxError, Service};

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct MembershipMessage {
    pub sender_id: String,
    pub message: Option<Vec<String>>,
}

impl MembershipMessage {
    pub fn new(sender_id: String, members: Option<Vec<String>>) -> Self {
        MembershipMessage {
            sender_id,
            message: members,
        }
    }
}

#[derive(Clone)]
pub struct Membership {
    sender_id: String,
    state: State,
}

impl Membership {
    pub fn new(node_id: String, state: State) -> Self {
        Membership {
            sender_id: node_id,
            state,
        }
    }
}

/// Service for handling Membership protocol.
impl Service<Message> for Membership {
    type Response = Option<Message>;
    type Error = BoxError;
    type Future = Pin<Box<dyn Future<Output = Result<Option<Message>, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    /// Membership doesn't respond with anything. It is pushed by a
    /// node when anyone connects to it.
    fn call(&mut self, msg: Message) -> Self::Future {
        let state = self.state.clone();
        async move {
            match msg {
                Message::Unicast(Unicast::Membership(MembershipMessage { message, sender_id })) => {
                    match message {
                        None => {
                            let members = state
                                .membership_handle
                                .get_members()
                                .await
                                .unwrap()
                                .into_keys()
                                .collect();
                            Ok(Some(
                                MembershipMessage {
                                    sender_id,
                                    message: Some(members),
                                }
                                .into(),
                            ))
                        }
                        _ => Ok(None),
                    }
                }
                _ => Ok(None),
            }
        }
        .boxed()
    }
}

#[cfg(test)]
mod membership_tests {

    use super::Membership;
    use crate::node::protocol::message_id_generator::MessageIdGenerator;
    use crate::node::protocol::MembershipMessage;
    #[mockall_double::double]
    use crate::node::reliable_sender::ReliableSenderHandle;
    use crate::node::MembershipHandle;
    use crate::node::State;
    use tower::{Service, ServiceExt};

    #[tokio::test]
    async fn it_should_create_membership_as_service_and_respond_to_default_with_membership() {
        let message_id_generator = MessageIdGenerator::new("localhost".to_string());
        let membership_handle = MembershipHandle::start("localhost".to_string()).await;
        let mut reliable_sender_handle = ReliableSenderHandle::default();
        reliable_sender_handle
            .expect_clone()
            .returning(ReliableSenderHandle::default);
        let _ = membership_handle
            .add_member("a".into(), reliable_sender_handle)
            .await;
        let state = State::new(membership_handle, message_id_generator);

        let mut p = Membership::new("local".to_string(), state);
        let res = p
            .ready()
            .await
            .unwrap()
            .call(
                MembershipMessage {
                    sender_id: "local".into(),
                    message: None,
                }
                .into(),
            )
            .await
            .unwrap();
        assert_eq!(
            res,
            Some(MembershipMessage::new("local".to_string(), Some(vec!["a".to_string()])).into())
        );
    }

    #[tokio::test]
    async fn it_should_create_membership_as_service_and_respond_to_membership_with_none() {
        let message_id_generator = MessageIdGenerator::new("localhost".to_string());
        let membership_handle = MembershipHandle::start("localhost".to_string()).await;
        let state = State::new(membership_handle, message_id_generator);

        let mut p = Membership::new("local".to_string(), state);
        let res = p
            .ready()
            .await
            .unwrap()
            .call(MembershipMessage::new("local".to_string(), Some(vec!["a".to_string()])).into())
            .await
            .unwrap();
        assert!(res.is_none());
    }
}
