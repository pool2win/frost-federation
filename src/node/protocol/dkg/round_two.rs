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
    pub async fn send_round2_packages(
        &self,
        round2_packages: Round2Map,
    ) -> Result<(usize, usize), BoxError> {
        let members = self.state.membership_handle.get_members().await?;
        if members.len() != round2_packages.len() {
            log::error!(
                "Members: {:?} != Round2 packages: {:?}",
                members.len(),
                round2_packages.len()
            );
            return Err("Members and round2 packages length mismatch".into());
        }
        // Collect all the futures into a Vec
        let send_futures: Vec<_> = members
            .into_iter()
            .zip(round2_packages.iter())
            .map(|((member_id, reliable_sender), (_, package))| {
                let message = PackageMessage::new(self.sender_id.clone(), Some(package.clone()));
                let message = Message::Unicast(Unicast::DKGRoundTwoPackage(message));
                log::debug!("Queueing send to member: {:?}", member_id);
                reliable_sender.send(message)
            })
            .collect();

        // Wait for all futures to complete, collecting errors
        let results = futures::future::join_all(send_futures).await;
        // Count successes and failures
        let (successes, failures): (usize, usize) =
            results.iter().fold((0, 0), |(s, f), result| match result {
                Ok(_) => (s + 1, f),
                Err(_) => (s, f + 1),
            });
        log::debug!(
            "Round2 package sends: {} succeeded, {} failed",
            successes,
            failures
        );

        // Check if any sends failed. We will change this to the threshold later
        if failures > 0 {
            log::error!("One or more round2 package sends failed");
            return Err("Failed to send some round2 packages".into());
        }
        Ok((successes, failures))
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
                    message: None, // message is None, so we build a new round2 package and send it
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
                Message::Unicast(Unicast::DKGRoundTwoPackage(PackageMessage {
                    sender_id: from_sender_id,
                    message: Some(message), // received a message
                })) => {
                    // Received round2 message and save it in state
                    let identifier = frost::Identifier::derive(from_sender_id.as_bytes()).unwrap();
                    state
                        .dkg_state
                        .add_round2_package(identifier, message)
                        .await
                        .unwrap();
                    Ok(None)
                }
                _ => {
                    log::error!(
                        "Not a Unicast message {:?}. Should not happen, but we need to match all message types",
                        msg
                    );
                    Ok(None)
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
    #[mockall_double::double]
    use crate::node::reliable_sender::ReliableSenderHandle;
    use crate::node::test_helpers::support::{build_membership, build_round2_state};
    use node::MembershipHandle;

    #[tokio::test]
    async fn test_build_round2_packages_insufficient_packages() {
        let membership_handle = build_membership(3).await;
        let state = node::state::State::new(
            membership_handle,
            MessageIdGenerator::new("local".to_string()),
        );
        let result = build_round2_packages("localhost".to_string(), state)
            .await
            .unwrap_err();
        assert_eq!(result, frost::Error::InvalidSecretShare);
    }

    #[tokio::test]
    async fn test_build_round2_packages_should_succeed() {
        let membership_handle = MembershipHandle::start("localhost".to_string()).await;
        for i in 1..3 {
            let mut mock_reliable_sender = ReliableSenderHandle::default();
            mock_reliable_sender.expect_clone().returning(|| {
                let mut mock = ReliableSenderHandle::default();
                mock.expect_clone().returning(ReliableSenderHandle::default);
                mock
            });
            let _ = membership_handle
                .add_member(format!("localhost{}", i), mock_reliable_sender)
                .await;
        }
        let state = node::state::State::new(
            membership_handle,
            MessageIdGenerator::new("local".to_string()),
        );
        let (state, round1_packages) = build_round2_state(state).await;

        // add all round1 packages to state
        for (id, package) in round1_packages {
            state
                .dkg_state
                .add_round1_package(id, package)
                .await
                .unwrap();
        }

        let result = build_round2_packages("localhost".to_string(), state).await;
        assert!(result.is_ok());
        let (round2_secret, round2_packages) = result.unwrap();
        assert_eq!(round2_packages.len(), 2);
    }

    #[tokio::test]
    async fn test_send_round2_packages_without_errors() {
        let _ = env_logger::try_init();
        let membership_handle = MembershipHandle::start("localhost".to_string()).await;
        for i in 1..3 {
            let mut mock_reliable_sender = ReliableSenderHandle::default();
            mock_reliable_sender.expect_clone().returning(|| {
                let mut mock = ReliableSenderHandle::default();
                mock.expect_clone().returning(ReliableSenderHandle::default);
                mock.expect_send()
                    //.times(1)
                    .returning(|_| futures::future::ok(()).boxed());
                mock
            });
            let _ = membership_handle
                .add_member(format!("localhost{}", i), mock_reliable_sender)
                .await;
        }
        let state = node::state::State::new(
            membership_handle,
            MessageIdGenerator::new("local".to_string()),
        );
        let (state, round1_packages) = build_round2_state(state).await;

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

    #[tokio::test]
    async fn test_send_round2_packages_with_error() {
        let _ = env_logger::try_init();

        let membership_handle = MembershipHandle::start("localhost".to_string()).await;

        let state = node::state::State::new(
            membership_handle.clone(),
            MessageIdGenerator::new("local".to_string()),
        );

        let mut mock_reliable_sender = ReliableSenderHandle::default();
        mock_reliable_sender.expect_clone().returning(|| {
            let mut mock = ReliableSenderHandle::default();
            mock.expect_clone().returning(ReliableSenderHandle::default);
            mock.expect_send()
                //.times(1)
                .return_once(|_| futures::future::ok(()).boxed());
            mock
        });
        let _ = membership_handle
            .add_member("localhost1".to_string(), mock_reliable_sender)
            .await;

        // Make one of the reliable senders return an error
        let mut mock_reliable_sender = ReliableSenderHandle::default();
        mock_reliable_sender.expect_clone().returning(|| {
            let mut mock = ReliableSenderHandle::default();
            mock.expect_clone().returning(ReliableSenderHandle::default);
            mock.expect_send()
                //.times(1)
                .return_once(|_| futures::future::err("Some error".into()).boxed());
            mock
        });

        let (mut state, round1_packages) = build_round2_state(state).await;

        state
            .membership_handle
            .add_member("localhost2".to_string(), mock_reliable_sender)
            .await
            .unwrap();

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
            .await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn test_add_received_round2_package() {
        let _ = env_logger::try_init();

        let membership_handle = MembershipHandle::start("localhost".to_string()).await;

        let state = node::state::State::new(
            membership_handle.clone(),
            MessageIdGenerator::new("localhost".to_string()),
        );

        for i in 1..3 {
            let mut mock_reliable_sender = ReliableSenderHandle::default();
            mock_reliable_sender.expect_clone().returning(|| {
                let mut mock = ReliableSenderHandle::default();
                mock.expect_clone().returning(ReliableSenderHandle::default);
                mock.expect_send()
                    //.times(1)
                    .returning(|_| futures::future::ok(()).boxed());
                mock
            });
            let _ = membership_handle
                .add_member(format!("localhost{}", i), mock_reliable_sender)
                .await;
        }

        let (mut state, round1_packages) = build_round2_state(state).await;

        // add all round1 packages to state
        for (identifier, package) in round1_packages {
            state
                .dkg_state
                .add_round1_package(identifier, package)
                .await
                .unwrap();
        }

        let (round2_secret_package, round2_map) =
            build_round2_packages("localhost".to_string(), state.clone())
                .await
                .unwrap();
        let (identifier, message) = round2_map.iter().next().unwrap();

        // call the package service to handle received round2 packages
        let mut pkg = Package::new("localhost".to_string(), state.clone());
        let res = pkg
            .call(Message::Unicast(Unicast::DKGRoundTwoPackage(
                PackageMessage::new("localhost1".to_string(), Some(message.clone())),
            )))
            .await;
        assert!(res.is_ok());

        let received_round2_package = state
            .dkg_state
            .get_received_round2_packages()
            .await
            .unwrap()
            .keys()
            .next()
            .unwrap()
            .clone();
        assert!(round2_map.contains_key(&received_round2_package));
    }
}
