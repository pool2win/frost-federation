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
        }
    }
}

/// Message for state handle to actor communication
pub(crate) enum StateMessage {
    /// Add a received round1 package to state
    AddRound1Package(dkg::round1::Package, oneshot::Sender<()>),
}

pub(crate) struct Actor {
    state: State,
    receiver: mpsc::Receiver<StateMessage>,
}

impl Actor {
    pub fn start(receiver: mpsc::Receiver<StateMessage>) -> Self {
        Self {
            state: State::new(),
            receiver,
        }
    }

    pub fn start_new_dkg(&mut self, respond_to: oneshot::Sender<()>) {
        self.state.in_progress = true;
        let _ = respond_to.send(());
    }

    pub fn add_round1_package(
        &mut self,
        identifier: frost::Identifier,
        package: dkg::round1::Package,
        respond_to: oneshot::Sender<()>,
    ) {
        self.state
            .received_round1_packages
            .insert(identifier, package);
        let _ = respond_to.send(());
    }
}

#[derive(Clone, Debug)]
pub(crate) struct StateHandle {
    sender: mpsc::Sender<StateMessage>,
}

impl StateHandle {
    /// Create a new state handle
    pub fn new(sender: mpsc::Sender<StateMessage>) -> Self {
        Self { sender }
    }

    /// Add round1 package to state
    pub async fn add_round1_package(&self, identifier: Identifier, package: dkg::round1::Package, respond_to: oneshot::Sender<()>) {
        let message = StateMessage::AddRound1Package(package, respond_to);
        let _ = self.sender.send(message).await;
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
    }

    #[test]
    fn test_actor_start() {
        let (tx, rx) = mpsc::channel(1);
        let mut actor = Actor::start(rx);
        assert_eq!(actor.state.in_progress, false);
    }

    #[test]
    fn test_actor_start_new_dkg() {
        let (tx, rx) = mpsc::channel(1);
        let mut actor = Actor::start(rx);
        let (tx1, rx1) = oneshot::channel();
        actor.start_new_dkg(tx1);
        assert_eq!(actor.state.in_progress, true);
    }

    #[test]
    fn test_actor_add_round1_package() {
        let (_tx, rx) = mpsc::channel(1);
        let mut actor = Actor::start(rx);
        let identifier = frost::Identifier::derive(b"1").unwrap();
        let rng = thread_rng();

        let (_secret_package, package) = frost::keys::dkg::part1(
            identifier,
            3 as u16,
            2 as u16,
            rng,
        ).unwrap();

        let (tx1, _rx1) = oneshot::channel();
        actor.add_round1_package(identifier, package, tx1);
        assert_eq!(actor.state.received_round1_packages.len(), 1);
    }
}
