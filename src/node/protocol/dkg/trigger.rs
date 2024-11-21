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

use crate::node::echo_broadcast::service::EchoBroadcast;
#[mockall_double::double]
use crate::node::echo_broadcast::EchoBroadcastHandle;
use crate::node::protocol::{dkg, Protocol};
#[mockall_double::double]
use crate::node::reliable_sender::ReliableSenderHandle;
use crate::node::State;
use tokio::time::{Duration, Instant};
use tower::ServiceExt;

pub async fn run_dkg_trigger(
    duration_millis: u64,
    node_id: String,
    state: State,
    echo_broadcast_handle: EchoBroadcastHandle,
    reliable_sender_handle: ReliableSenderHandle,
) {
    let period = Duration::from_millis(duration_millis);
    loop {
        let start = Instant::now() + period;
        let mut interval = tokio::time::interval_at(start, period);
        interval.tick().await;

        trigger_dkg_round_one(
            node_id.clone(),
            state.clone(),
            echo_broadcast_handle.clone(),
            reliable_sender_handle.clone(),
        )
        .await;
    }
}

pub(crate) async fn trigger_dkg_round_one(
    node_id: String,
    state: State,
    echo_broadcast_handle: EchoBroadcastHandle,
    reliable_sender_handle: ReliableSenderHandle,
) {
    let protocol_service = Protocol::new(node_id.clone(), state.clone(), reliable_sender_handle);
    let echo_broadcast_service = EchoBroadcast::new(
        protocol_service,
        echo_broadcast_handle,
        state,
        node_id.clone(),
    );

    log::debug!("Sending DKG echo broadcast");

    let _ = echo_broadcast_service
        .oneshot(dkg::round_one::PackageMessage::new(node_id, None).into())
        .await;

    log::debug!("DKG Echo broadcast finished");
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{self, timeout};

    #[mockall_double::double]
    use crate::node::echo_broadcast::EchoBroadcastHandle;
    use crate::node::membership::MembershipHandle;
    use crate::node::protocol::message_id_generator::MessageIdGenerator;
    #[mockall_double::double]
    use crate::node::reliable_sender::ReliableSenderHandle;
    use tokio::time::Duration;

    #[tokio::test]
    async fn test_trigger_fires() {
        let interval = time::interval(Duration::from_millis(100));
        let node_id = "test-node".to_string();
        let membership_handle = MembershipHandle::start("local".into()).await;

        let state = State::new(membership_handle, MessageIdGenerator::new("local".into()));
        let mut mock_echo_broadcast_handle = EchoBroadcastHandle::default();
        mock_echo_broadcast_handle.expect_clone().returning(|| {
            let mut mock = EchoBroadcastHandle::default();
            mock.expect_clone().returning(|| {
                let mut mocked = EchoBroadcastHandle::default();
                mocked.expect_send().return_once(|_, _| Ok(()));
                mocked
            });
            mock
        });

        let mut mock_reliable_sender_handle = ReliableSenderHandle::default();
        mock_reliable_sender_handle.expect_clone().returning(|| {
            let mut mock = ReliableSenderHandle::default();
            mock.expect_clone().returning(|| {
                let mut mocked = ReliableSenderHandle::default();
                mocked
                    .expect_clone()
                    .returning(|| ReliableSenderHandle::default());
                mocked
            });
            mock
        });

        // Wait for just over one interval to ensure we get at least one trigger
        let result: Result<(), time::error::Elapsed> = timeout(
            Duration::from_millis(110),
            run_dkg_trigger(
                15,
                node_id,
                state,
                mock_echo_broadcast_handle,
                mock_reliable_sender_handle,
            ),
        )
        .await;

        assert!(result.is_err(), "Trigger should not complete");
    }
}
