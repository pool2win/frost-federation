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
use crate::node::State;
use crate::node::{echo_broadcast::service::EchoBroadcast, protocol::Message};
use frost_secp256k1 as frost;
use tokio::sync::mpsc;
use tokio::time::Duration;
use tower::{BoxError, ServiceExt};
use tracing::{debug, error, info};

/// Timeout in seconds, for DKG rounds one and two.
const DKG_ROUND_TIMEOUT: u64 = 10;

/// Runs the DKG trigger loop.
/// This will trigger the DKG round one protocol after an initial wait.
pub async fn run_dkg_trigger(
    node_id: String,
    mut state: State,
    echo_broadcast_handle: EchoBroadcastHandle,
    round_one_rx: &mut mpsc::Receiver<()>,
    round_two_rx: &mut mpsc::Receiver<()>,
    trigger_dkg_rx: &mut mpsc::Receiver<()>,
) {
    while let Some(_) = trigger_dkg_rx.recv().await {
        state.update_expected_members().await;
        let result = run_dkg(
            node_id.clone(),
            state.clone(),
            echo_broadcast_handle.clone(),
            round_one_rx,
            round_two_rx,
        )
        .await;

        if let Err(e) = result {
            error!("DKG trigger failed with error {:?}", e);
            return;
        } else {
            info!("DKG trigger finished");
        }
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
    info!("Sending DKG echo broadcast");
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
pub(crate) async fn run_dkg(
    node_id: String,
    state: State,
    echo_broadcast_handle: EchoBroadcastHandle,
    round_one_rx: &mut mpsc::Receiver<()>,
    round_two_rx: &mut mpsc::Receiver<()>,
) -> Result<(), BoxError> {
    let protocol_service: Protocol = Protocol::new(node_id.clone(), state.clone(), None);

    let round1_future = build_round1_future(
        node_id.clone(),
        protocol_service.clone(),
        echo_broadcast_handle,
        state.clone(),
    );

    let round2_future = build_round2_future(node_id.clone(), protocol_service.clone());

    // TODO Improve this to allow round1 to finish as soon as all other parties have sent their round1 message
    // This will mean moving the timeout into round1 service

    // Start round1
    if let Err(e) = round1_future.await {
        error!("Error running round 1: {:?}", e);
        return Err("Error running round 1: failed with error".into());
    }
    round_one_rx.recv().await.unwrap();
    info!("Round 1 finished");

    // start round2
    if let Err(e) = round2_future.await {
        error!("Error running round 2: {:?}", e);
        return Err("Error running round 2: failed with error".into());
    }
    round_two_rx.recv().await.unwrap();
    info!("Round 2 finished");

    // Get packages required to run part3
    match build_key_packages(&state).await {
        Ok((key_package, public_key_package)) => {
            info!(
                "DKG part3 finished. key_package = {:?}, pubkey_package = {:?}",
                key_package, public_key_package
            );
            state.dkg_state.set_key_package(key_package).await?;
            state
                .dkg_state
                .set_public_key_package(public_key_package)
                .await?;
            Ok(())
        }
        Err(e) => {
            error!("Error running DKG part3: {:?}", e);
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

    debug!(
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

        let (_round_one_tx, mut round_one_rx) = mpsc::channel::<()>(1);
        let (_round_two_tx, mut round_two_rx) = mpsc::channel::<()>(1);
        let (_trigger_dkg_tx, mut trigger_dkg_rx) = mpsc::channel::<()>(1);
        // Wait for just over one interval to ensure we get at least one trigger
        let result: Result<(), time::error::Elapsed> = timeout(
            Duration::from_millis(10),
            run_dkg_trigger(
                node_id,
                state,
                mock_echo_broadcast_handle,
                &mut round_one_rx,
                &mut round_two_rx,
                &mut trigger_dkg_rx,
            ),
        )
        .await;

        assert!(result.is_err(), "Trigger should not complete");
    }
}
