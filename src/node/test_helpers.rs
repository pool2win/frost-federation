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
}
