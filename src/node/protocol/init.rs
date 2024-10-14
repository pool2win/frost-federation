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
use crate::node::protocol::{MembershipMessage, Protocol, RoundOnePackageMessage};
use crate::node::reliable_sender::service::ReliableSend;
#[mockall_double::double]
use crate::node::reliable_sender::ReliableSenderHandle;
use crate::node::State;

use tower::{Layer, ServiceExt};

/// Run initial protocols for Node
pub(crate) async fn initialize(
    node_id: String,
    state: State,
    echo_broadcast_handle: EchoBroadcastHandle,
    reliable_sender_handle: ReliableSenderHandle,
    delivery_time: u64,
) {
    // let handshake_service = protocol::Protocol::new(node_id.clone(), self.state.clone());
    // let reliable_sender_service =
    //     ReliableSend::new(handshake_service, reliable_sender_handle.clone());
    // let timeout_layer = tower::timeout::TimeoutLayer::new(
    //     tokio::time::Duration::from_millis(self.delivery_timeout),
    // );
    // let _ = timeout_layer
    //     .layer(reliable_sender_service)
    //     .oneshot(HandshakeMessage::default().into())
    //     .await;

    // log::info!("Handshake finished");

    let round_one_service = Protocol::new(node_id.clone(), state.clone());
    let echo_broadcast_service = EchoBroadcast::new(
        round_one_service,
        echo_broadcast_handle,
        state.clone(),
        node_id.clone(),
    );

    log::info!("Sending echo broadcast");

    let _ = echo_broadcast_service
        .oneshot(
            RoundOnePackageMessage::new(node_id.clone(), "hello from round one package".into())
                .into(),
        )
        .await;

    log::info!("Echo broadcast finished");
    let _ = send_membership(
        node_id.clone(),
        reliable_sender_handle,
        state.clone(),
        delivery_time,
    )
    .await;

    log::info!("Membership sent");
}

pub(crate) async fn send_membership(
    node_id: String,
    sender: ReliableSenderHandle,
    state: State,
    delivery_time: u64,
) {
    log::info!("Sending membership information");
    let protocol_service = Protocol::new(node_id.clone(), state);
    let reliable_sender_service = ReliableSend::new(protocol_service, sender);
    let timeout_layer =
        tower::timeout::TimeoutLayer::new(tokio::time::Duration::from_millis(delivery_time));
    let res = timeout_layer
        .layer(reliable_sender_service)
        .oneshot(MembershipMessage::new(node_id, None).into())
        .await;
    log::debug!("Membership sending result {:?}", res);
}
