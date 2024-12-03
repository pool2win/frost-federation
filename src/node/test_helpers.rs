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
    use std::collections::BTreeMap;
    use tracing::debug;

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
        debug!("Secret package {:?}", secret_package);

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

    /// Test support function, hand crafted to run DKG with three parties.
    pub fn setup_test_dkg_packages() -> (frost::keys::KeyPackage, frost::keys::PublicKeyPackage) {
        let mut rng = thread_rng();
        let identifier_a = frost::Identifier::derive(b"localhost_a").unwrap();
        let identifier_b = frost::Identifier::derive(b"localhost_b").unwrap();
        let identifier_c = frost::Identifier::derive(b"localhost_c").unwrap();

        // Generate round 1 packages
        let (secret_package_a, package_a) =
            frost::keys::dkg::part1(identifier_a, 3, 2, &mut rng).unwrap();

        let (secret_package_b, package_b) =
            frost::keys::dkg::part1(identifier_b, 3, 2, &mut rng).unwrap();

        let (secret_package_c, package_c) =
            frost::keys::dkg::part1(identifier_c, 3, 2, &mut rng).unwrap();

        // Create round 1 packages map (only B and C's packages for A)
        let mut round1_packages_a = BTreeMap::new();
        round1_packages_a.insert(identifier_b, package_b.clone());
        round1_packages_a.insert(identifier_c, package_c.clone());

        let mut round1_packages_b = BTreeMap::new();
        round1_packages_b.insert(identifier_a, package_a.clone());
        round1_packages_b.insert(identifier_c, package_c.clone());

        let mut round1_packages_c = BTreeMap::new();
        round1_packages_c.insert(identifier_a, package_a.clone());
        round1_packages_c.insert(identifier_b, package_b);

        // Generate round 2 packages for all participants
        let (round2_secret_package_a, round2_packages_a) =
            frost::keys::dkg::part2(secret_package_a, &round1_packages_a).unwrap();
        let (round2_secret_package_b, round2_packages_b) =
            frost::keys::dkg::part2(secret_package_b, &round1_packages_b).unwrap();
        let (round2_secret_package_c, round2_packages_c) =
            frost::keys::dkg::part2(secret_package_c, &round1_packages_c).unwrap();

        // Create a map of all round 2 packages
        let mut all_round2_packages_a = BTreeMap::new();
        all_round2_packages_a.insert(identifier_b, round2_packages_b[&identifier_a].clone());
        all_round2_packages_a.insert(identifier_c, round2_packages_c[&identifier_a].clone());

        // Now generate the final key package
        frost::keys::dkg::part3(
            &round2_secret_package_a,
            &round1_packages_a,
            &all_round2_packages_a,
        )
        .unwrap()
    }
}
