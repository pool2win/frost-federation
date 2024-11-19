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

type Round1Map = BTreeMap<frost::Identifier, dkg::round1::Package>;

pub(crate) struct State {
    pub in_progress: bool,
    pub pub_key: Option<frost::keys::PublicKeyPackage>,
    pub received_round1_packages: Round1Map,
    pub secret_package: Option<frost::keys::dkg::round1::SecretPackage>,
}

/// Track state of DKG.
/// We explicitly start DKG here and then check for termination.
/// We also store the latest key generated here - which is not cleared out till the next key is finalised
/// Question - should we track the membership of senders engaged in this DKG too?
impl State {
    pub fn new() -> Self {
        Self {
            in_progress: false,
            pub_key: None,
            received_round1_packages: Round1Map::new(),
            secret_package: None,
        }
    }
}

/// Message for state handle to actor communication
pub(crate) enum StateMessage {
    /// Add a received round1 package to state
    AddRound1Package(frost::Identifier, dkg::round1::Package, oneshot::Sender<()>),
    /// Add a received secret package to state
    AddSecretPackage(frost::keys::dkg::round1::SecretPackage, oneshot::Sender<()>),
    /// Get a received secret package from state
    GetSecretPackage(oneshot::Sender<Option<frost::keys::dkg::round1::SecretPackage>>),
    /// Get received round1 packages from state
    GetReceivedRound1Packages(oneshot::Sender<Round1Map>),
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
                StateMessage::AddSecretPackage(secret_package, respond_to) => {
                    self.add_secret_package(secret_package, respond_to);
                }
                StateMessage::GetSecretPackage(respond_to) => {
                    let secret_package = self.state.secret_package.clone();
                    let _ = respond_to.send(secret_package);
                }
                StateMessage::GetReceivedRound1Packages(respond_to) => {
                    let received_round1_packages = self.state.received_round1_packages.clone();
                    let _ = respond_to.send(received_round1_packages);
                }
            }
        }
    }

    fn add_round1_package(
        &mut self,
        identifier: Identifier,
        package: dkg::round1::Package,
        respond_to: oneshot::Sender<()>,
    ) {
        self.state
            .received_round1_packages
            .insert(identifier, package);
        let _ = respond_to.send(());
    }

    fn add_secret_package(
        &mut self,
        secret_package: frost::keys::dkg::round1::SecretPackage,
        respond_to: oneshot::Sender<()>,
    ) {
        self.state.secret_package = Some(secret_package);
        let _ = respond_to.send(());
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
    ) -> Result<(), oneshot::error::RecvError> {
        let (tx, rx) = oneshot::channel();
        let message = StateMessage::AddRound1Package(identifier, package, tx);
        let _ = self.sender.send(message).await;
        rx.await
    }

    /// Add secret package to state
    pub async fn add_secret_package(
        &self,
        secret_package: frost::keys::dkg::round1::SecretPackage,
    ) -> Result<(), oneshot::error::RecvError> {
        let (tx, rx) = oneshot::channel();
        let message = StateMessage::AddSecretPackage(secret_package, tx);
        let _ = self.sender.send(message).await;
        rx.await
    }

    /// Get a received secret package from state
    pub async fn get_secret_package(
        &self,
    ) -> Result<Option<frost::keys::dkg::round1::SecretPackage>, oneshot::error::RecvError> {
        let (tx, rx) = oneshot::channel();
        let message = StateMessage::GetSecretPackage(tx);
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
}

#[cfg(test)]
mod dkg_state_tests {
    use super::*;
    use rand::thread_rng;
    use std::collections::BTreeMap;
    use tokio::sync::oneshot;

    #[test]
    fn test_state_new() {
        let state = State::new();
        assert_eq!(state.in_progress, false);
        assert_eq!(state.pub_key, None);
        assert_eq!(state.received_round1_packages, BTreeMap::new());
        assert_eq!(state.secret_package, None);
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

    #[test]
    fn test_actor_add_round1_package() {
        let (_tx, rx) = mpsc::channel(1);
        let mut actor = Actor::new(rx);
        let identifier = frost::Identifier::derive(b"1").unwrap();
        let rng = thread_rng();

        let (_secret_package, package) =
            frost::keys::dkg::part1(identifier, 3 as u16, 2 as u16, rng).unwrap();

        let (tx1, _rx1) = oneshot::channel();
        actor.add_round1_package(identifier, package, tx1);
        assert_eq!(actor.state.received_round1_packages.len(), 1);
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
        assert_eq!(actor.state.secret_package, Some(secret_package));
    }
}

#[cfg(test)]
mod dkg_state_handle_tests {
    use super::*;
    use rand::thread_rng;

    #[tokio::test]
    async fn test_state_handle_new() {
        let handle = StateHandle::new();
        assert!(handle.sender.capacity() > 0);
    }

    #[tokio::test]
    async fn test_state_handle_add_round1_package() {
        let state_handle = StateHandle::new();
        let identifier = frost::Identifier::derive(b"1").unwrap();
        let (_secret_package, package) =
            frost::keys::dkg::part1(identifier, 3, 2, thread_rng()).unwrap();

        // Send the package and assert success
        assert!(state_handle
            .add_round1_package(identifier, package.clone())
            .await
            .is_ok());
        let received_round1_packages = state_handle.get_received_round1_packages().await.unwrap();
        assert_eq!(received_round1_packages.len(), 1);
    }

    #[tokio::test]
    async fn test_state_handle_add_secret_package() {
        let state_handle = StateHandle::new();

        let identifier = frost::Identifier::derive(b"1").unwrap();
        let (secret_package, _package) =
            frost::keys::dkg::part1(identifier, 3, 2, thread_rng()).unwrap();

        // Send the secret package and assert success
        assert!(state_handle
            .add_secret_package(secret_package.clone())
            .await
            .is_ok());
        let secret_package = state_handle.get_secret_package().await.unwrap().unwrap();
        assert_eq!(secret_package, secret_package);
    }
}
