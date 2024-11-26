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

use std::result;

use crate::node::membership::MembershipHandle;
use crate::node::protocol::dkg;
use crate::node::protocol::message_id_generator::MessageIdGenerator;

/// Handlers to query/update node state
#[derive(Clone)]
pub(crate) struct State {
    pub(crate) membership_handle: MembershipHandle,
    pub(crate) message_id_generator: MessageIdGenerator,
    pub(crate) dkg_state: dkg::state::StateHandle,
}

impl State {
    pub async fn new(
        membership_handle: MembershipHandle,
        message_id_generator: MessageIdGenerator,
    ) -> Self {
        let num_members = membership_handle.get_members().await.unwrap().len();
        Self {
            membership_handle,
            message_id_generator,
            dkg_state: dkg::state::StateHandle::new(Some(num_members)),
        }
    }

    /// Starts a new DKG round by setting the expected number of members
    /// based on the current membership size
    pub async fn start_new_dkg(&mut self) {
        let num_members = self.membership_handle.get_members().await.unwrap().len();
        log::info!("Starting DKG with membership = {}", num_members);
        self.dkg_state = dkg::state::StateHandle::new(Some(num_members));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[mockall_double::double]
    use crate::node::reliable_sender::ReliableSenderHandle;

    #[tokio::test]
    async fn test_start_new_dkg_sets_expected_members() {
        let membership_handle = MembershipHandle::start("localhost".to_string()).await;
        let mut mock_reliable_sender = ReliableSenderHandle::default();
        mock_reliable_sender
            .expect_clone()
            .returning(ReliableSenderHandle::default);
        let _ = membership_handle
            .add_member("localhost_1".to_string(), mock_reliable_sender)
            .await;

        let message_id_generator = MessageIdGenerator::new("localhost".to_string());
        let mut state = State::new(membership_handle, message_id_generator).await;

        state.start_new_dkg().await;

        assert_eq!(state.dkg_state.expected_members, Some(1));
    }
}
