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

/// Runs the DKG trigger loop.
/// This will trigger the DKG round one protocol at a given interval.
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

/// Triggers the DKG round one protocol.
/// This will return once the round package has been successfully sent to all members.
pub(crate) async fn trigger_dkg_round_one(
    node_id: String,
    state: State,
    echo_broadcast_handle: EchoBroadcastHandle,
    reliable_sender_handle: ReliableSenderHandle,
) {
    let protocol_service: Protocol =
        Protocol::new(node_id.clone(), state.clone(), reliable_sender_handle);
    let echo_broadcast_service = EchoBroadcast::new(
        protocol_service.clone(),
        echo_broadcast_handle,
        state,
        node_id.clone(),
    );

    log::info!("Sending DKG echo broadcast");
    let echo_broadcast_timeout_service = tower::ServiceBuilder::new()
        .timeout(Duration::from_secs(10))
        .service(echo_broadcast_service);

    let _ = echo_broadcast_timeout_service
        .oneshot(dkg::round_one::PackageMessage::new(node_id.clone(), None).into())
        .await;
    log::info!("DKG Echo broadcast finished");

    // TODO[DKG]: We need to check if round one successfully finised.

    log::info!("Sending DKG round two message");
    let round_two_timeout_service = tower::ServiceBuilder::new()
        .timeout(Duration::from_secs(10))
        .service(protocol_service);

    let _ = round_two_timeout_service
        .oneshot(dkg::round_two::PackageMessage::new(node_id, None).into())
        .await;
    log::info!("DKG round two message finished");
}

#[cfg(test)]
mod dkg_trigger_tests {
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
                mocked.expect_clone().returning(|| {
                    let mut mocked = ReliableSenderHandle::default();
                    mocked
                        .expect_clone()
                        .returning(|| ReliableSenderHandle::default());
                    mocked
                });
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
