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

#[mockall_double::double]
use crate::node::echo_broadcast::EchoBroadcastHandle;
use crate::node::protocol::{dkg, Protocol};
#[mockall_double::double]
use crate::node::reliable_sender::ReliableSenderHandle;
use crate::node::State;
use crate::node::{echo_broadcast::service::EchoBroadcast, protocol::Message};
use frost_secp256k1 as frost;
use tokio::sync::mpsc;
use tokio::time::{Duration, Instant};
use tower::{BoxError, ServiceExt};

/// Timeout in seconds, for DKG rounds one and two.
const DKG_ROUND_TIMEOUT: u64 = 10;

/// Runs the DKG trigger loop.
/// This will trigger the DKG round one protocol after an initial wait.
pub async fn run_dkg_trigger(
    duration_millis: u64,
    node_id: String,
    mut state: State,
    echo_broadcast_handle: EchoBroadcastHandle,
    reliable_sender_handle: Option<ReliableSenderHandle>,
    round_one_rx: mpsc::Receiver<()>,
    round_two_rx: mpsc::Receiver<()>,
) {
    let period = Duration::from_millis(duration_millis);
    let start = Instant::now() + period;
    let mut interval = tokio::time::interval_at(start, period);
    interval.tick().await; // Using tick here so we can later run this in a loop

    state.update_expected_members().await;

    let result = trigger_dkg(
        node_id.clone(),
        state.clone(),
        echo_broadcast_handle.clone(),
        reliable_sender_handle.clone(),
        round_one_rx,
        round_two_rx,
    )
    .await;

    if let Err(e) = result {
        log::error!("DKG trigger failed with error {:?}", e);
        return;
    } else {
        log::info!("DKG trigger finished");
    }
}

/// Build round1 future that the trigger function can use to compose the DKG protocol
/// This wraps the EchoBroadcast service which further wraps the round_one::Package service.
fn build_round1_future(
    node_id: String,
    protocol_service: Protocol,
    echo_broadcast_handle: EchoBroadcastHandle,
    state: State,
) -> impl std::future::Future<Output = Result<(), tower::BoxError>> {
    let echo_broadcast_service = EchoBroadcast::new(
        protocol_service.clone(),
        echo_broadcast_handle,
        state,
        node_id.clone(),
    );

    // Build round1 service as future
    log::info!("Sending DKG echo broadcast");
    let echo_broadcast_timeout_service = tower::ServiceBuilder::new()
        .timeout(Duration::from_secs(DKG_ROUND_TIMEOUT))
        .service(echo_broadcast_service);

    echo_broadcast_timeout_service
        .oneshot(dkg::round_one::PackageMessage::new(node_id, None).into())
}

/// Build round2 future for use in trigger
/// This wraps the round_two::Package service into a tower timeout service
fn build_round2_future(
    node_id: String,
    protocol_service: Protocol,
) -> impl std::future::Future<Output = Result<Option<Message>, tower::BoxError>> {
    // Build round2 service as future
    let round_two_timeout_service = tower::ServiceBuilder::new()
        .timeout(Duration::from_secs(DKG_ROUND_TIMEOUT))
        .service(protocol_service);

    round_two_timeout_service.oneshot(dkg::round_two::PackageMessage::new(node_id, None).into())
}

/// Triggers the DKG round one protocol.
/// This will return once the round package has been successfully sent to all members.
pub(crate) async fn trigger_dkg(
    node_id: String,
    state: State,
    echo_broadcast_handle: EchoBroadcastHandle,
    reliable_sender_handle: Option<ReliableSenderHandle>,
    mut round_one_rx: mpsc::Receiver<()>,
    mut round_two_rx: mpsc::Receiver<()>,
) -> Result<(), BoxError> {
    let protocol_service: Protocol =
        Protocol::new(node_id.clone(), state.clone(), reliable_sender_handle);

    let round1_future = build_round1_future(
        node_id.clone(),
        protocol_service.clone(),
        echo_broadcast_handle,
        state.clone(),
    );

    let round2_future = build_round2_future(node_id.clone(), protocol_service.clone());

    // TODO Improve this to allow round1 to finish as soon as all other parties have sent their round1 message
    // This will mean moving the timeout into round1 service

    // Wait for round1 to finish, give it 5 seconds
    if round1_future.await.is_err() {
        log::error!("Error running round 1");
        return Err("Error running round 1".into());
    }
    round_one_rx.recv().await.unwrap();
    log::info!("Round 1 finished");

    log::debug!(
        "received round1 packages = {:?}",
        state
            .dkg_state
            .get_received_round1_packages()
            .await
            .unwrap()
    );

    // start round2
    if round2_future.await.is_err() {
        log::error!("Error running round 2");
        return Err("Error running round 2".into());
    }
    round_two_rx.recv().await.unwrap();
    log::info!("Round 2 finished");

    // Get packages required to run part3
    match build_key_packages(&state).await {
        Ok((key_package, public_key_package)) => {
            log::info!(
                "DKG part3 finished. key_package = {:?}, pubkey_package = {:?}",
                key_package,
                public_key_package
            );
            state.dkg_state.set_key_package(key_package).await?;
            state
                .dkg_state
                .set_public_key_package(public_key_package)
                .await?;
            Ok(())
        }
        Err(e) => {
            log::error!("Error running DKG part3: {:?}", e);
            Err(e.into())
        }
    }
}

/// Build final key packages using frost dkg round3
/// Get all the required packages from node::state::dkg_state and then build the key package.
/// The function returns any error as they are, so they should be handled by the caller.
async fn build_key_packages(
    state: &State,
) -> Result<(frost::keys::KeyPackage, frost::keys::PublicKeyPackage), BoxError> {
    let round2_secret_package = state
        .dkg_state
        .get_round2_secret_package()
        .await?
        .ok_or("No round2 secret package")?;
    let round1_packages = state.dkg_state.get_received_round1_packages().await?;
    let round2_packages = state.dkg_state.get_received_round2_packages().await?;

    log::debug!(
        "round1_packages = {:?}, round2_packages = {:?}",
        round1_packages.len(),
        round2_packages.len()
    );

    frost::keys::dkg::part3(&round2_secret_package, &round1_packages, &round2_packages)
        .map_err(|e| e.into())
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

        let state = State::new(membership_handle, MessageIdGenerator::new("local".into())).await;
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

        let (round_one_tx, round_one_rx) = mpsc::channel::<()>(1);
        let (round_two_tx, round_two_rx) = mpsc::channel::<()>(1);

        // Wait for just over one interval to ensure we get at least one trigger
        let result: Result<(), time::error::Elapsed> = timeout(
            Duration::from_millis(10),
            run_dkg_trigger(
                15,
                node_id,
                state,
                mock_echo_broadcast_handle,
                Some(mock_reliable_sender_handle),
                round_one_rx,
                round_two_rx,
            ),
        )
        .await;

        assert!(result.is_err(), "Trigger should not complete");
    }
}
