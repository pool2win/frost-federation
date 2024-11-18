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

use crate::node::protocol::BroadcastProtocol;
use crate::node::protocol::Message;
use crate::node::state::State;
use frost_secp256k1 as frost;
use futures::{Future, FutureExt};
use rand::thread_rng;
use serde::{Deserialize, Serialize};
use std::pin::Pin;
use std::task::{Context, Poll};
use tower::{BoxError, Service};

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct PackageMessage {
    pub sender_id: String,
    pub message: Option<frost::keys::dkg::round1::Package>,
}

impl PackageMessage {
    pub fn new(sender_id: String, message: Option<frost::keys::dkg::round1::Package>) -> Self {
        PackageMessage { sender_id, message }
    }
}

async fn build_round1_package(
    sender_id: String,
    state: crate::node::state::State,
) -> Result<Message, frost::Error> {
    let max_min_signers = state
        .membership_handle
        .get_members()
        .await
        .map(|members| {
            let num_members = members.len();
            (num_members, (num_members * 2).div_ceil(3))
        })
        .unwrap();
    let participant_identifier = frost::Identifier::derive(sender_id.as_bytes()).unwrap();
    let rng = thread_rng();
    log::debug!("SIGNERS: {} {}", max_min_signers.0, max_min_signers.1);
    let result = frost::keys::dkg::part1(
        participant_identifier,
        max_min_signers.0 as u16,
        max_min_signers.1 as u16,
        rng,
    );
    match result {
        Ok((secret_package, round1_package)) => Ok(Message::Broadcast(
            BroadcastProtocol::DKGRoundOnePackage(PackageMessage::new(
                sender_id,
                Some(round1_package),
            )),
            Some(state.message_id_generator.next()),
        )),
        Err(e) => Err(e),
    }
}

#[derive(Clone)]
pub struct Package {
    sender_id: String,
    state: State,
}

impl Package {
    pub fn new(node_id: String, state: State) -> Self {
        Package {
            sender_id: node_id,
            state,
        }
    }
}

/// service for handling Package protocol.
///
/// By making all protocol into a Service, we can use tower:Steer to
/// multiplex across services.
impl Service<Message> for Package {
    type Response = Option<Message>;
    type Error = BoxError;
    type Future = Pin<Box<dyn Future<Output = Result<Option<Message>, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    /// Builds and sends any messages required in response to
    /// receiving round one package
    /// For now, there is no response.
    fn call(&mut self, msg: Message) -> Self::Future {
        let state = self.state.clone();
        let sender_id = self.sender_id.clone();
        async move {
            let response = build_round1_package(sender_id, state).await?;
            Ok(Some(response))
        }
        .boxed()
    }
}

#[cfg(test)]
mod round_one_package_tests {

    use super::build_round1_package;
    use super::Package;
    use crate::node::protocol::message_id_generator::MessageId;
    use crate::node::protocol::BroadcastProtocol;
    use crate::node::protocol::NetworkMessage;
    use crate::node::protocol::{dkg::round_one::PackageMessage, Message};
    use crate::node::state::State;
    use crate::node::test_helpers::support::build_membership;
    use crate::node::MessageIdGenerator;
    use frost_secp256k1 as frost;
    use tower::{Service, ServiceExt};

    #[tokio::test]
    async fn it_should_create_round_one_package_as_service_and_respond_to_round_one_package_with_none(
    ) {
        let message_id_generator = MessageIdGenerator::new("localhost".to_string());
        let membership_handle = build_membership(3).await;
        let state = State::new(membership_handle, message_id_generator);

        let mut pkg = Package::new("local".into(), state);
        let res = pkg
            .ready()
            .await
            .unwrap()
            .call(Message::Broadcast(
                BroadcastProtocol::DKGRoundOnePackage(PackageMessage::new(
                    "local".to_string(),
                    None,
                )),
                Some(MessageId(1)),
            ))
            .await
            .unwrap();
        assert!(res.is_some());
        assert_eq!(res.unwrap().get_sender_id(), "local");
    }
    #[tokio::test]
    async fn it_should_serialize_and_deserialize_round_one_public_key_package() {
        let message_id_generator = MessageIdGenerator::new("localhost".to_string());
        let membership_handle = build_membership(3).await;
        let state = State::new(membership_handle, message_id_generator);

        let round1_package = build_round1_package("local".into(), state).await.unwrap();

        // Extract the public key package from the NetworkMessage
        if let Message::Broadcast(BroadcastProtocol::DKGRoundOnePackage(pkg_msg), _message_id) =
            round1_package
        {
            let public_key_package = pkg_msg.message.unwrap();

            let serialized = public_key_package.serialize().unwrap();
            let deserialized = frost::keys::dkg::round1::Package::deserialize(&serialized).unwrap();

            assert_eq!(public_key_package, deserialized);
        } else {
            panic!("Expected DKGRoundOnePackage");
        }
    }
}
