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

use crate::node::dkg::state::Round2Map;
use crate::node::protocol::dkg::get_max_min_signers;
use crate::node::protocol::Message;
use crate::node::{self, protocol::Unicast};
use frost_secp256k1 as frost;
use futures::FutureExt;
use serde::{Deserialize, Serialize};
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};
use tower::{BoxError, Service};

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct PackageMessage {
    pub sender_id: String,
    pub message: Option<frost::keys::dkg::round2::Package>,
}

impl PackageMessage {
    pub fn new(sender_id: String, message: Option<frost::keys::dkg::round2::Package>) -> Self {
        PackageMessage { sender_id, message }
    }
}

#[derive(Clone)]
pub struct Package {
    sender_id: String,
    state: node::State,
}

impl Package {
    pub fn new(sender_id: String, state: node::State) -> Self {
        Package { sender_id, state }
    }

    /// Send round2 packages to all members using reliable sender
    pub async fn send_round2_packages(&self, round2_packages: Round2Map) -> Result<(), BoxError> {
        let members = self.state.membership_handle.get_members().await?;
        for (member_id, reliable_sender) in members {
            // Derive identifier from member id
            let identifier = frost::Identifier::derive(member_id.as_bytes())?;

            // Get corresponding round2 package for this member
            if let Some(package) = round2_packages.get(&identifier) {
                // Create package message
                let message = PackageMessage::new(self.sender_id.clone(), Some(package.clone()));
                // Wrap package message in Message::Unicast
                let message = Message::Unicast(Unicast::DKGRoundTwoPackage(message));
                // Send via reliable sender
                if reliable_sender.send(message).await.is_err() {
                    return Err("Failed to send round2 package".into());
                }
            }
        }
        Ok(())
    }
}

impl Service<Message> for Package {
    type Response = Option<Message>;
    type Error = BoxError;
    type Future = Pin<Box<dyn Future<Output = Result<Option<Message>, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, msg: Message) -> Self::Future {
        let this = self.clone();
        let sender_id = this.sender_id.clone();
        let state = this.state.clone();

        async move {
            match msg {
                Message::Unicast(Unicast::DKGRoundTwoPackage(PackageMessage {
                    sender_id: _,
                    message: None, // message is None, so we build a new round2 package
                })) => {
                    match build_round2_packages(sender_id, state.clone()).await {
                        Ok((round2_secret_package, round2_packages)) => {
                            // Store the round2 secret package
                            if let Err(e) = state
                                .dkg_state
                                .add_round2_secret_package(round2_secret_package)
                                .await
                            {
                                log::error!("Failed to store round2 secret package: {:?}", e);
                                return Err(e.into());
                            }
                            // Send round2 packages to all members
                            if let Err(e) = this.send_round2_packages(round2_packages).await {
                                log::error!("Failed to send round2 packages: {:?}", e);
                                return Err(e.into());
                            }
                            log::debug!("Sent round2 packages");
                            Ok(None)
                        }
                        Err(e) => {
                            log::error!("Failed to build round2 packages: {:?}", e);
                            Err(e.into())
                        }
                    }
                }
                _ => {
                    // Received round2 message
                    todo!()
                }
            }
        }
        .boxed()
    }
}

/// Builds a round two package for this node's sender id
/// Uses the stored secret package and received round1 packages to build a round2 package
/// Returns None if not enough round1 packages have been received
pub async fn build_round2_packages(
    sender_id: String,
    state: crate::node::state::State,
) -> Result<(frost::keys::dkg::round2::SecretPackage, Round2Map), frost::Error> {
    let (max_signers, min_signers) = get_max_min_signers(&state).await;
    log::debug!("ROUND2: SIGNERS: {} {}", max_signers, min_signers);

    let secret_package = match state.dkg_state.get_round1_secret_package().await.unwrap() {
        Some(package) => package,
        None => return Err(frost::Error::InvalidSecretShare),
    };

    let received_packages = state
        .dkg_state
        .get_received_round1_packages()
        .await
        .unwrap();
    log::debug!("Received round1 packages: {:?}", received_packages.len());

    // We need at least min_signers to proceed
    if received_packages.len() < min_signers {
        return Err(frost::Error::InvalidMinSigners);
    }

    let (round2_secret, round2_packages) =
        frost::keys::dkg::part2(secret_package, &received_packages)?;
    Ok((round2_secret, round2_packages))
}

#[cfg(test)]
mod round_two_tests {
    use super::*;
    use crate::node::protocol::message_id_generator::MessageIdGenerator;
    use crate::node::test_helpers::support::build_membership;
    use node::dkg::state::Round1Map;
    use rand::thread_rng;

    #[tokio::test]
    async fn test_build_round2_packages_insufficient_packages() {
        let membership_handle = build_membership(3).await;
        let state = node::state::State::new(
            membership_handle,
            MessageIdGenerator::new("local".to_string()),
        );
        let result = build_round2_packages("node1".to_string(), state)
            .await
            .unwrap_err();
        assert_eq!(result, frost::Error::InvalidSecretShare);
    }

    async fn build_round2_state(num_nodes: usize) -> (node::state::State, Round1Map) {
        let membership_handle = build_membership(num_nodes).await;
        let state = node::state::State::new(
            membership_handle,
            MessageIdGenerator::new("local".to_string()),
        );

        let rng = thread_rng();
        let mut round1_packages = Round1Map::new();

        // generate our round1 secret and package
        let (secret_package, round1_package) = frost::keys::dkg::part1(
            frost::Identifier::derive(b"node1").unwrap(),
            3,
            2,
            rng.clone(),
        )
        .unwrap();
        log::debug!("Secret package {:?}", secret_package);

        // add our secret package to state
        state
            .dkg_state
            .add_round1_secret_package(secret_package)
            .await
            .unwrap();

        // Add packages for other nodes
        let (_, round1_package2) = frost::keys::dkg::part1(
            frost::Identifier::derive(b"node2").unwrap(),
            3,
            2,
            rng.clone(),
        )
        .unwrap();
        round1_packages.insert(
            frost::Identifier::derive(b"node2").unwrap(),
            round1_package2,
        );

        let (_, round1_package3) = frost::keys::dkg::part1(
            frost::Identifier::derive(b"node3").unwrap(),
            3,
            2,
            rng.clone(),
        )
        .unwrap();
        round1_packages.insert(
            frost::Identifier::derive(b"node3").unwrap(),
            round1_package3,
        );
        (state, round1_packages)
    }

    #[tokio::test]
    async fn test_build_round2_packages_should_succeed() {
        let (state, round1_packages) = build_round2_state(3).await;

        // add all round1 packages to state
        for (id, package) in round1_packages {
            state
                .dkg_state
                .add_round1_package(id, package)
                .await
                .unwrap();
        }

        let result = build_round2_packages("node1".to_string(), state).await;
        assert!(result.is_ok());
        let (round2_secret, round2_packages) = result.unwrap();
        assert_eq!(round2_packages.len(), 2);
    }

    #[tokio::test]
    async fn test_send_round2_packages_without_errors() {
        let _ = env_logger::try_init();
        let (state, round1_packages) = build_round2_state(3).await;

        // add all round1 packages to state
        for (identifier, package) in round1_packages {
            state
                .dkg_state
                .add_round1_package(identifier, package)
                .await
                .unwrap();
        }

        // call the package service to send round2 packages
        let mut pkg = Package::new("local".into(), state);
        let res = pkg
            .call(Message::Unicast(Unicast::DKGRoundTwoPackage(
                PackageMessage::new("local".to_string(), None),
            )))
            .await
            .unwrap();
        assert!(res.is_none());
    }
}
