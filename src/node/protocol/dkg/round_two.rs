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

use crate::node;
use crate::node::protocol::Message;
use crate::node::protocol::Unicast;
use frost_secp256k1 as frost;
use serde::{Deserialize, Serialize};
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};
use tower::Service;

use super::state::Round2Map;

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
}

impl Service<Message> for Package {
    type Response = Option<Message>;
    type Error = Box<dyn std::error::Error + Send + Sync>;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, _req: Message) -> Self::Future {
        // Implementation will be provided later
        todo!()
    }
}

/// Builds a round two package for this node's sender id
/// Uses the stored secret package and received round1 packages to build a round2 package
/// Returns None if not enough round1 packages have been received
pub async fn build_round2_packages(
    sender_id: String,
    state: crate::node::state::State,
) -> Result<Round2Map, frost::Error> {
    let max_min_signers = state
        .membership_handle
        .get_members()
        .await
        .map(|members| {
            let num_members = members.len();
            (num_members, (num_members * 2).div_ceil(3))
        })
        .unwrap();

    let secret_package = match state.dkg_state.get_secret_package().await.unwrap() {
        Some(package) => package,
        None => return Err(frost::Error::InvalidSecretShare),
    };

    let received_packages = state
        .dkg_state
        .get_received_round1_packages()
        .await
        .unwrap();

    if received_packages.len() < max_min_signers.1 {
        return Err(frost::Error::InvalidMinSigners);
    }

    let (round2_secret, round2_packages) =
        frost::keys::dkg::part2(secret_package, &received_packages)?;
    Ok(round2_packages)
}

#[cfg(test)]
mod round_two_tests {
    use super::*;
    use crate::node::test_helpers::support::build_membership;

    use crate::node::protocol::message_id_generator::MessageIdGenerator;

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

    #[tokio::test]
    async fn test_package_message_creation() {
        let sender_id = "node1".to_string();
        let package_msg = PackageMessage::new(sender_id.clone(), None);

        assert_eq!(package_msg.sender_id, sender_id);
        assert!(package_msg.message.is_none());
    }
}