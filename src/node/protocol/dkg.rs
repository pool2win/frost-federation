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

pub(crate) mod round_one;
pub(crate) mod round_two;
pub(crate) mod state;
pub(crate) mod trigger;

use crate::node::state::State;

/// Get the max and min signers for the DKG
pub(crate) async fn get_max_min_signers(state: &State) -> (usize, usize) {
    let members = state.membership_handle.get_members().await.unwrap();
    let num_members = members.len() + 1;
    (num_members, (num_members * 2).div_ceil(3))
}
#[cfg(test)]
mod tests {
    use frost_secp256k1 as frost;

    #[test]
    /// A test to check that the derive function is deterministic. Keeping it for future reference.
    fn test_identifier_derive() {
        let id1 = frost::Identifier::derive(b"test_node").unwrap();
        let id2 = frost::Identifier::derive(b"test_node").unwrap();
        assert_eq!(id1, id2);

        let id3 = frost::Identifier::derive(b"different_node").unwrap();
        assert_ne!(id1, id3);
    }
}
