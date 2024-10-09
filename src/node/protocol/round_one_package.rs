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

use crate::node::protocol::{Broadcast, Message};
use crate::node::state::State;
use futures::{Future, FutureExt};
use serde::{Deserialize, Serialize};
use std::pin::Pin;
use std::task::{Context, Poll};
use tower::{BoxError, Service};

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Default)]
pub struct RoundOnePackageMessage {
    pub sender_id: String,
    pub message: String,
}

impl RoundOnePackageMessage {
    pub fn new(sender_id: String, message: String) -> Self {
        RoundOnePackageMessage { sender_id, message }
    }
}

#[derive(Clone)]
pub struct RoundOnePackage {
    sender_id: String,
    state: State,
}

impl RoundOnePackage {
    pub fn new(node_id: String, state: State) -> Self {
        RoundOnePackage {
            sender_id: node_id,
            state,
        }
    }
}

/// Service for handling RoundOnePackage protocol.
///
/// By making all protocol into a Service, we can use tower:Steer to
/// multiplex across services.
impl Service<Message> for RoundOnePackage {
    type Response = Option<Message>;
    type Error = BoxError;
    type Future = Pin<Box<dyn Future<Output = Result<Option<Message>, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, msg: Message) -> Self::Future {
        let state = self.state.clone();
        async move {
            match msg {
                // We receive the package and don't send anything back.
                Message::BroadcastMessage(Broadcast::RoundOnePackage(_m, Some(_mid))) => Ok(None),
                // For the first instance of the message, add a message_id
                Message::BroadcastMessage(Broadcast::RoundOnePackage(m, None)) => {
                    Ok(Some(Message::BroadcastMessage(Broadcast::RoundOnePackage(
                        m,
                        Some(state.message_id_generator.next()),
                    ))))
                }
                // We shouldn't receive round one package as a unicast
                Message::UnicastMessage(_) => Err("Round one package should not be unicast".into()),
            }
        }
        .boxed()
    }
}

#[cfg(test)]
mod round_one_package_tests {

    use super::RoundOnePackage;
    use crate::node::protocol::message_id_generator::MessageId;
    use crate::node::protocol::{Message, RoundOnePackageMessage};
    use crate::node::state::State;
    use crate::node::MessageIdGenerator;
    use crate::node::{membership::MembershipHandle, protocol::Broadcast};
    use tower::{Service, ServiceExt};

    #[tokio::test]
    async fn it_should_create_round_one_package_as_service_and_respond_to_none_with_round_one_package(
    ) {
        let message_id_generator = MessageIdGenerator::new("localhost".to_string());
        let membership_handle = MembershipHandle::start("localhost".to_string()).await;
        let state = State::new(membership_handle, message_id_generator);

        let mut p = RoundOnePackage::new("local".into(), state);
        let res = p
            .ready()
            .await
            .unwrap()
            .call(RoundOnePackageMessage::default().into())
            .await
            .unwrap();
        assert!(res.is_some());
        let Message::BroadcastMessage(Broadcast::RoundOnePackage(m, mid)) = res.unwrap() else {
            todo!("nothing here");
        };
        assert_eq!(
            m,
            RoundOnePackageMessage::new("".to_string(), "".to_string())
        );
    }

    #[tokio::test]
    async fn it_should_create_round_one_package_as_service_and_respond_to_round_one_package_with_none(
    ) {
        let message_id_generator = MessageIdGenerator::new("localhost".to_string());
        let membership_handle = MembershipHandle::start("localhost".to_string()).await;
        let state = State::new(membership_handle, message_id_generator);

        let mut p = RoundOnePackage::new("local".into(), state);
        let res = p
            .ready()
            .await
            .unwrap()
            .call(Message::BroadcastMessage(Broadcast::RoundOnePackage(
                RoundOnePackageMessage::new("local".to_string(), "round_one_package".to_string()),
                Some(MessageId(1)),
            )))
            .await
            .unwrap();
        assert!(res.is_none());
    }

    #[test]
    fn it_should_create_default_round_one_package_message() {
        assert_eq!(RoundOnePackageMessage::default().sender_id, "".to_string())
    }
}
