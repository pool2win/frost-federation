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

#[cfg(test)]
pub(crate) mod support {
    use crate::node::membership::MembershipHandle;
    #[mockall_double::double]
    use crate::node::reliable_sender::ReliableSenderHandle;
    use crate::node::{self, protocol::dkg::state::Round1Map};
    use frost_secp256k1 as frost;
    use rand::thread_rng;

    /// Builds a membership with the given number of nodes
    /// Do not add the local node to the membership, therefore it loops from 1 to num
    pub(crate) async fn build_membership(num: usize) -> MembershipHandle {
        let membership_handle = MembershipHandle::start("localhost".to_string()).await;
        for i in 1..num {
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
        membership_handle
    }

    pub async fn build_round2_state(state: node::state::State) -> (node::state::State, Round1Map) {
        let rng = thread_rng();
        let mut round1_packages = Round1Map::new();

        // generate our round1 secret and package
        let (secret_package, round1_package) = frost::keys::dkg::part1(
            frost::Identifier::derive(b"localhost").unwrap(),
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
            frost::Identifier::derive(b"localhost1").unwrap(),
            3,
            2,
            rng.clone(),
        )
        .unwrap();
        round1_packages.insert(
            frost::Identifier::derive(b"localhost1").unwrap(),
            round1_package2,
        );

        let (_, round1_package3) = frost::keys::dkg::part1(
            frost::Identifier::derive(b"localhost2").unwrap(),
            3,
            2,
            rng.clone(),
        )
        .unwrap();
        round1_packages.insert(
            frost::Identifier::derive(b"localhost2").unwrap(),
            round1_package3,
        );
        (state, round1_packages)
    }
}
