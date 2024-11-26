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

use frost::keys::dkg;
use frost_secp256k1::{self as frost, Identifier};
use std::collections::BTreeMap;
use tokio::sync::{mpsc, oneshot};

pub(crate) type Round1Map = BTreeMap<frost::Identifier, dkg::round1::Package>;
pub(crate) type Round2Map = BTreeMap<frost::Identifier, frost::keys::dkg::round2::Package>;
pub(crate) struct State {
    pub in_progress: bool,
    pub received_round1_packages: Round1Map,
    pub received_round2_packages: Round2Map,
    pub round1_secret_package: Option<frost::keys::dkg::round1::SecretPackage>,
    pub round2_secret_package: Option<frost::keys::dkg::round2::SecretPackage>,
    pub key_package: Option<frost::keys::KeyPackage>,
    pub public_key_package: Option<frost::keys::PublicKeyPackage>,
    pub expected_members: usize,
}

/// Track state of DKG.
/// We explicitly start DKG here and then check for termination.
/// We also store the latest key generated here - which is not cleared out till the next key is finalised
/// Question - should we track the membership of senders engaged in this DKG too?
impl State {
    pub fn new() -> Self {
        Self {
            in_progress: false,
            received_round1_packages: Round1Map::new(),
            received_round2_packages: Round2Map::new(),
            round1_secret_package: None,
            round2_secret_package: None,
            key_package: None,
            public_key_package: None,
            expected_members: 0,
        }
    }
}

/// Message for state handle to actor communication
pub(crate) enum StateMessage {
    /// Add a received round1 package to state
    AddRound1Package(
        frost::Identifier,
        dkg::round1::Package,
        oneshot::Sender<bool>,
    ),

    /// Add a received secret package to state
    AddRound1SecretPackage(frost::keys::dkg::round1::SecretPackage, oneshot::Sender<()>),

    /// Get a received secret package from state
    GetRound1SecretPackage(oneshot::Sender<Option<frost::keys::dkg::round1::SecretPackage>>),

    /// Get received round1 packages from state
    GetReceivedRound1Packages(oneshot::Sender<Round1Map>),

    /// Add a received round2 secret package to state
    AddRound2SecretPackage(frost::keys::dkg::round2::SecretPackage, oneshot::Sender<()>),

    /// Get a received round2 secret package from state
    GetRound2SecretPackage(oneshot::Sender<Option<frost::keys::dkg::round2::SecretPackage>>),

    /// Get a received round2 packages from state
    GetReceivedRound2Packages(oneshot::Sender<Round2Map>),

    /// Add a received round2 package to state
    AddRound2Package(
        frost::Identifier,
        frost::keys::dkg::round2::Package,
        oneshot::Sender<()>,
    ),

    /// Set the key package
    SetKeyPackage(frost::keys::KeyPackage, oneshot::Sender<()>),

    /// Get the key package
    GetKeyPackage(oneshot::Sender<Option<frost::keys::KeyPackage>>),

    /// Set the public key package
    SetPublicKeyPackage(frost::keys::PublicKeyPackage, oneshot::Sender<()>),

    /// Get the public key package
    GetPublicKeyPackage(oneshot::Sender<Option<frost::keys::PublicKeyPackage>>),
}

pub(crate) struct Actor {
    state: State,
    receiver: mpsc::Receiver<StateMessage>,
}

impl Actor {
    fn new(receiver: mpsc::Receiver<StateMessage>) -> Self {
        Self {
            state: State::new(),
            receiver,
        }
    }

    // Add a new method to run the actor
    async fn run(&mut self) {
        while let Some(message) = self.receiver.recv().await {
            match message {
                StateMessage::AddRound1Package(identifier, package, respond_to) => {
                    self.add_round1_package(identifier, package, respond_to);
                }
                StateMessage::AddRound1SecretPackage(secret_package, respond_to) => {
                    self.add_secret_package(secret_package, respond_to);
                }
                StateMessage::GetRound1SecretPackage(respond_to) => {
                    let secret_package = self.state.round1_secret_package.clone();
                    let _ = respond_to.send(secret_package);
                }
                StateMessage::GetReceivedRound1Packages(respond_to) => {
                    let received_round1_packages = self.state.received_round1_packages.clone();
                    let _ = respond_to.send(received_round1_packages);
                }
                StateMessage::AddRound2SecretPackage(secret_package, respond_to) => {
                    self.add_round2_secret_package(secret_package, respond_to);
                }
                StateMessage::GetRound2SecretPackage(respond_to) => {
                    self.get_round2_secret_package(respond_to);
                }
                StateMessage::GetReceivedRound2Packages(respond_to) => {
                    let received_round2_packages = self.state.received_round2_packages.clone();
                    let _ = respond_to.send(received_round2_packages);
                }
                StateMessage::AddRound2Package(identifier, package, respond_to) => {
                    self.add_round2_package(identifier, package, respond_to);
                }
                StateMessage::SetKeyPackage(key_package, respond_to) => {
                    self.set_key_package(key_package, respond_to);
                }
                StateMessage::GetKeyPackage(respond_to) => {
                    self.get_key_package(respond_to);
                }
                StateMessage::SetPublicKeyPackage(public_key_package, respond_to) => {
                    self.set_public_key_package(public_key_package, respond_to);
                }
                StateMessage::GetPublicKeyPackage(respond_to) => {
                    self.get_public_key_package(respond_to);
                }
            }
        }
    }

    fn add_round1_package(
        &mut self,
        identifier: Identifier,
        package: dkg::round1::Package,
        respond_to: oneshot::Sender<bool>,
    ) {
        self.state
            .received_round1_packages
            .insert(identifier, package);
        let received_count = self.state.received_round1_packages.len();
        let _ = respond_to.send(received_count == self.state.expected_members);
    }

    fn add_secret_package(
        &mut self,
        secret_package: frost::keys::dkg::round1::SecretPackage,
        respond_to: oneshot::Sender<()>,
    ) {
        self.state.round1_secret_package = Some(secret_package);
        let _ = respond_to.send(());
    }

    fn add_round2_secret_package(
        &mut self,
        secret_package: frost::keys::dkg::round2::SecretPackage,
        respond_to: oneshot::Sender<()>,
    ) {
        self.state.round2_secret_package = Some(secret_package);
        let _ = respond_to.send(());
    }

    fn get_round2_secret_package(
        &self,
        respond_to: oneshot::Sender<Option<frost::keys::dkg::round2::SecretPackage>>,
    ) {
        let secret_package = self.state.round2_secret_package.clone();
        let _ = respond_to.send(secret_package);
    }

    fn add_round2_package(
        &mut self,
        identifier: Identifier,
        package: frost::keys::dkg::round2::Package,
        respond_to: oneshot::Sender<()>,
    ) {
        self.state
            .received_round2_packages
            .insert(identifier, package);
        let _ = respond_to.send(());
    }

    fn get_received_round2_packages(&self, respond_to: oneshot::Sender<Round2Map>) {
        let received_round2_packages = self.state.received_round2_packages.clone();
        let _ = respond_to.send(received_round2_packages);
    }

    fn set_key_package(
        &mut self,
        key_package: frost::keys::KeyPackage,
        respond_to: oneshot::Sender<()>,
    ) {
        self.state.key_package = Some(key_package);
        let _ = respond_to.send(());
    }

    fn get_key_package(&self, respond_to: oneshot::Sender<Option<frost::keys::KeyPackage>>) {
        let key_package = self.state.key_package.clone();
        let _ = respond_to.send(key_package);
    }

    fn set_public_key_package(
        &mut self,
        public_key_package: frost::keys::PublicKeyPackage,
        respond_to: oneshot::Sender<()>,
    ) {
        self.state.public_key_package = Some(public_key_package);
        let _ = respond_to.send(());
    }

    fn get_public_key_package(
        &self,
        respond_to: oneshot::Sender<Option<frost::keys::PublicKeyPackage>>,
    ) {
        let public_key_package = self.state.public_key_package.clone();
        let _ = respond_to.send(public_key_package);
    }
}

#[derive(Clone, Debug)]
pub(crate) struct StateHandle {
    sender: mpsc::Sender<StateMessage>,
}

impl StateHandle {
    /// Create a new state handle and spawn the actor
    pub fn new() -> Self {
        let (sender, receiver) = mpsc::channel(1);
        let mut actor = Actor::new(receiver);

        // Spawn the actor task
        tokio::spawn(async move {
            actor.run().await;
        });

        Self { sender }
    }

    /// Add round1 package to state
    pub async fn add_round1_package(
        &self,
        identifier: Identifier,
        package: dkg::round1::Package,
    ) -> Result<bool, oneshot::error::RecvError> {
        let (tx, rx) = oneshot::channel();
        let message = StateMessage::AddRound1Package(identifier, package, tx);
        let _ = self.sender.send(message).await;
        rx.await
    }

    /// Add secret package to state
    pub async fn add_round1_secret_package(
        &self,
        secret_package: frost::keys::dkg::round1::SecretPackage,
    ) -> Result<(), oneshot::error::RecvError> {
        let (tx, rx) = oneshot::channel();
        let message = StateMessage::AddRound1SecretPackage(secret_package, tx);
        let _ = self.sender.send(message).await;
        rx.await
    }

    /// Get a received secret package from state
    pub async fn get_round1_secret_package(
        &self,
    ) -> Result<Option<frost::keys::dkg::round1::SecretPackage>, oneshot::error::RecvError> {
        let (tx, rx) = oneshot::channel();
        let message = StateMessage::GetRound1SecretPackage(tx);
        let _ = self.sender.send(message).await;
        rx.await
    }

    /// Get a received round1 packages from state
    pub async fn get_received_round1_packages(
        &self,
    ) -> Result<Round1Map, oneshot::error::RecvError> {
        let (tx, rx) = oneshot::channel();
        let message = StateMessage::GetReceivedRound1Packages(tx);
        let _ = self.sender.send(message).await;
        rx.await
    }

    /// Add round2 secret package to state
    pub async fn add_round2_secret_package(
        &self,
        secret_package: frost::keys::dkg::round2::SecretPackage,
    ) -> Result<(), oneshot::error::RecvError> {
        let (tx, rx) = oneshot::channel();
        let message = StateMessage::AddRound2SecretPackage(secret_package, tx);
        let _ = self.sender.send(message).await;
        rx.await
    }

    /// Get a received round2 secret package from state
    pub async fn get_round2_secret_package(
        &self,
    ) -> Result<Option<frost::keys::dkg::round2::SecretPackage>, oneshot::error::RecvError> {
        let (tx, rx) = oneshot::channel();
        let message = StateMessage::GetRound2SecretPackage(tx);
        let _ = self.sender.send(message).await;
        rx.await
    }

    /// Get a received round2 packages from state
    pub async fn get_received_round2_packages(
        &self,
    ) -> Result<Round2Map, oneshot::error::RecvError> {
        let (tx, rx) = oneshot::channel();
        let message = StateMessage::GetReceivedRound2Packages(tx);
        let _ = self.sender.send(message).await;
        rx.await
    }

    /// Add a received round2 package to state
    pub async fn add_round2_package(
        &self,
        identifier: Identifier,
        package: frost::keys::dkg::round2::Package,
    ) -> Result<(), oneshot::error::RecvError> {
        let (tx, rx) = oneshot::channel();
        let message = StateMessage::AddRound2Package(identifier, package, tx);
        let _ = self.sender.send(message).await;
        rx.await
    }

    /// Set the key package
    pub async fn set_key_package(
        &self,
        key_package: frost::keys::KeyPackage,
    ) -> Result<(), oneshot::error::RecvError> {
        let (tx, rx) = oneshot::channel();
        let message = StateMessage::SetKeyPackage(key_package, tx);
        let _ = self.sender.send(message).await;
        rx.await
    }

    /// Get the key package
    pub async fn get_key_package(
        &self,
    ) -> Result<Option<frost::keys::KeyPackage>, oneshot::error::RecvError> {
        let (tx, rx) = oneshot::channel();
        let message = StateMessage::GetKeyPackage(tx);
        let _ = self.sender.send(message).await;
        rx.await
    }

    /// Set the public key package
    pub async fn set_public_key_package(
        &self,
        public_key_package: frost::keys::PublicKeyPackage,
    ) -> Result<(), oneshot::error::RecvError> {
        let (tx, rx) = oneshot::channel();
        let message = StateMessage::SetPublicKeyPackage(public_key_package, tx);
        let _ = self.sender.send(message).await;
        rx.await
    }

    /// Get the public key package
    pub async fn get_public_key_package(
        &self,
    ) -> Result<Option<frost::keys::PublicKeyPackage>, oneshot::error::RecvError> {
        let (tx, rx) = oneshot::channel();
        let message = StateMessage::GetPublicKeyPackage(tx);
        let _ = self.sender.send(message).await;
        rx.await
    }
}

#[cfg(test)]
mod dkg_state_tests {
    use super::*;
    use crate::node::protocol::message_id_generator::MessageIdGenerator;
    #[mockall_double::double]
    use crate::node::reliable_sender::ReliableSenderHandle;
    use crate::node::{test_helpers::support::build_round2_state, MembershipHandle};
    use futures::FutureExt;
    use rand::thread_rng;
    use std::collections::BTreeMap;
    use tokio::sync::oneshot;

    #[test]
    fn test_state_new() {
        let state = State::new();
        assert_eq!(state.in_progress, false);
        assert_eq!(state.received_round1_packages, BTreeMap::new());
        assert_eq!(state.round1_secret_package, None);
        assert_eq!(state.round2_secret_package, None);
        assert_eq!(state.key_package, None);
        assert_eq!(state.public_key_package, None);
    }

    #[test]
    fn test_actor_start() {
        let (tx, rx) = mpsc::channel(1);
        let mut actor = Actor::new(rx);
        assert_eq!(actor.state.in_progress, false);
    }

    #[tokio::test]
    async fn test_actor_start_new_dkg() {
        let (tx, rx) = mpsc::channel(1);
        let mut actor = Actor::new(rx);
        assert_eq!(actor.state.in_progress, false);
    }

    #[tokio::test]
    async fn test_actor_add_round1_package() {
        let (_tx, rx) = mpsc::channel(1);
        let mut actor = Actor::new(rx);
        actor.state.expected_members = 2; // Set expected members
        let identifier1 = frost::Identifier::derive(b"1").unwrap();
        let identifier2 = frost::Identifier::derive(b"2").unwrap();
        let rng = thread_rng();

        // Create first package
        let (_secret_package1, package1) =
            frost::keys::dkg::part1(identifier1, 3 as u16, 2 as u16, rng.clone()).unwrap();

        // Add first package - should return false as we don't have all packages yet
        let (tx1, rx1) = oneshot::channel();
        actor.add_round1_package(identifier1, package1, tx1);
        assert_eq!(rx1.await.unwrap(), false);
        assert_eq!(actor.state.received_round1_packages.len(), 1);

        // Create and add second package - should return true as we now have all packages
        let (_secret_package2, package2) =
            frost::keys::dkg::part1(identifier2, 3 as u16, 2 as u16, rng).unwrap();
        let (tx2, rx2) = oneshot::channel();
        actor.add_round1_package(identifier2, package2, tx2);
        assert_eq!(rx2.await.unwrap(), true);
        assert_eq!(actor.state.received_round1_packages.len(), 2);
    }

    #[test]
    fn test_actor_add_secret_package() {
        let (_tx, rx) = mpsc::channel(1);
        let mut actor = Actor::new(rx);
        let identifier = frost::Identifier::derive(b"1").unwrap();
        let rng = thread_rng();

        let (secret_package, _package) =
            frost::keys::dkg::part1(identifier, 3 as u16, 2 as u16, rng).unwrap();

        let (tx1, _rx1) = oneshot::channel();
        actor.add_secret_package(secret_package.clone(), tx1);
        assert_eq!(actor.state.round1_secret_package, Some(secret_package));
    }

    #[tokio::test]
    async fn test_actor_add_round2_package() {
        let (_tx, rx) = mpsc::channel(1);
        let mut actor = Actor::new(rx);

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
        let state = crate::node::state::State::new(
            membership_handle,
            MessageIdGenerator::new("local".to_string()),
        );
        let (state, round1_packages) = build_round2_state(state).await;

        // Generate round2 packages
        let (round2_secret, round2_packages) = frost::keys::dkg::part2(
            state
                .dkg_state
                .get_round1_secret_package()
                .await
                .unwrap()
                .unwrap(),
            &round1_packages,
        )
        .unwrap();

        // Add each round2 package to state
        for (identifier, round2_package) in round2_packages.iter() {
            let (tx, _rx) = oneshot::channel();
            actor.add_round2_package(*identifier, round2_package.clone(), tx);
        }

        assert_eq!(actor.state.received_round2_packages.len(), 2);

        let (tx, rx) = oneshot::channel();
        actor.get_received_round2_packages(tx);
        let received_packages = rx.await.unwrap();
        assert_eq!(received_packages.len(), 2);
    }

    #[tokio::test]
    async fn test_actor_set_get_key_package() {
        let (_tx, rx) = mpsc::channel(1);
        let mut actor = Actor::new(rx);

        // Get test packages
        let (key_package, public_key_package) =
            crate::node::test_helpers::support::setup_test_dkg_packages();

        // Test setting
        let (tx1, _rx1) = oneshot::channel();
        actor.set_key_package(key_package.clone(), tx1);
        assert_eq!(actor.state.key_package, Some(key_package.clone()));

        // Test getting
        let (tx2, rx2) = oneshot::channel();
        actor.get_key_package(tx2);
        let retrieved_package = rx2.await.unwrap();
        assert_eq!(retrieved_package, Some(key_package));

        // Test setting
        let (tx1, _rx1) = oneshot::channel();
        actor.set_public_key_package(public_key_package.clone(), tx1);
        assert_eq!(
            actor.state.public_key_package,
            Some(public_key_package.clone())
        );

        // Test getting
        let (tx2, rx2) = oneshot::channel();
        actor.get_public_key_package(tx2);
        let retrieved_package = rx2.await.unwrap();
        assert_eq!(retrieved_package, Some(public_key_package));
    }
}

#[cfg(test)]
mod dkg_state_handle_tests {
    use super::*;
    use crate::node;
    use crate::node::protocol::message_id_generator::MessageIdGenerator;
    use crate::node::test_helpers::support::build_membership;
    use rand::thread_rng;

    #[tokio::test]
    async fn test_state_handle_new() {
        let handle = StateHandle::new();
        assert!(handle.sender.capacity() > 0);
    }

    #[tokio::test]
    async fn test_state_handle_add_round1_package() {
        let state_handle = StateHandle::new();

        // Set up identifiers and packages
        let identifier1 = frost::Identifier::derive(b"1").unwrap();
        let identifier2 = frost::Identifier::derive(b"2").unwrap();
        let (_secret_package1, package1) =
            frost::keys::dkg::part1(identifier1, 3, 2, thread_rng()).unwrap();
        let (_secret_package2, package2) =
            frost::keys::dkg::part1(identifier2, 3, 2, thread_rng()).unwrap();

        // First package should return false (not all packages received)
        let result = state_handle
            .add_round1_package(identifier1, package1)
            .await
            .unwrap();
        assert_eq!(result, false);

        let received_packages = state_handle.get_received_round1_packages().await.unwrap();
        assert_eq!(received_packages.len(), 1);

        // Second package should return true (all packages received)
        let result = state_handle
            .add_round1_package(identifier2, package2)
            .await
            .unwrap();
        assert_eq!(result, true);

        let received_packages = state_handle.get_received_round1_packages().await.unwrap();
        assert_eq!(received_packages.len(), 2);
    }

    #[tokio::test]
    async fn test_state_handle_add_secret_package() {
        let state_handle = StateHandle::new();

        let identifier = frost::Identifier::derive(b"1").unwrap();
        let (secret_package, _package) =
            frost::keys::dkg::part1(identifier, 3, 2, thread_rng()).unwrap();

        // Send the secret package and assert success
        assert!(state_handle
            .add_round1_secret_package(secret_package.clone())
            .await
            .is_ok());
        let secret_package = state_handle
            .get_round1_secret_package()
            .await
            .unwrap()
            .unwrap();
        assert_eq!(secret_package, secret_package);
    }

    #[tokio::test]
    async fn test_state_handle_add_round2_secret_package() {
        let membership_handle = build_membership(3).await;
        let state = node::state::State::new(
            membership_handle,
            MessageIdGenerator::new("local".to_string()),
        );

        let rng = thread_rng();
        let mut round1_packages = Round1Map::new();

        // generate our round1 secret and package
        let (secret_package, _round1_package) = frost::keys::dkg::part1(
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

        // add all round1 packages to state
        for (id, package) in round1_packages {
            state
                .dkg_state
                .add_round1_package(id, package)
                .await
                .unwrap();
        }

        let (round2_secret_package, _round2_package) =
            crate::node::protocol::dkg::round_two::build_round2_packages(
                "node1".to_string(),
                state.clone(),
            )
            .await
            .unwrap();

        // Send the secret package and assert success
        assert!(state
            .dkg_state
            .add_round2_secret_package(round2_secret_package.clone())
            .await
            .is_ok());
        let secret_package = state
            .dkg_state
            .get_round2_secret_package()
            .await
            .unwrap()
            .unwrap();
        assert_eq!(secret_package, secret_package);
    }

    #[tokio::test]
    async fn test_state_handle_set_get_key_package() {
        let state_handle = StateHandle::new();

        // Get test packages
        let (key_package, public_key_package) =
            crate::node::test_helpers::support::setup_test_dkg_packages();

        // Test setting
        assert!(state_handle
            .set_key_package(key_package.clone())
            .await
            .is_ok());

        // Test getting
        let retrieved_package = state_handle.get_key_package().await.unwrap();
        assert_eq!(retrieved_package, Some(key_package));
    }

    #[tokio::test]
    async fn test_state_handle_set_get_public_key_package() {
        let state_handle = StateHandle::new();

        // Get test packages
        let (key_package, public_key_package) =
            crate::node::test_helpers::support::setup_test_dkg_packages();

        // Test setting
        assert!(state_handle
            .set_public_key_package(public_key_package.clone())
            .await
            .is_ok());

        // Test getting
        let retrieved_package = state_handle.get_public_key_package().await.unwrap();
        assert_eq!(retrieved_package, Some(public_key_package));
    }
}
