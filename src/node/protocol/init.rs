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

use crate::node::protocol::{HandshakeMessage, MembershipMessage, Protocol};
use crate::node::reliable_sender::service::ReliableSend;
#[mockall_double::double]
use crate::node::reliable_sender::ReliableSenderHandle;
use crate::node::State;

use tokio::time::Duration;
use tower::{timeout::TimeoutLayer, Layer, ServiceExt};

/// Run initial protocols for Node
pub(crate) async fn initialize_handshake(
    node_id: String,
    state: State,
    reliable_sender_handle: ReliableSenderHandle,
    delivery_timeout: u64,
) {
    let handshake_service = Protocol::new(node_id, state, reliable_sender_handle.clone());
    let reliable_sender_service = ReliableSend::new(handshake_service, reliable_sender_handle);
    let timeout_layer = TimeoutLayer::new(Duration::from_millis(delivery_timeout));
    let _ = timeout_layer
        .layer(reliable_sender_service)
        .oneshot(HandshakeMessage::default().into())
        .await;

    log::info!("Handshake finished");
}

pub(crate) async fn send_membership(
    node_id: String,
    sender: ReliableSenderHandle,
    state: State,
    delivery_time: u64,
) {
    log::info!("Sending membership information");
    let protocol_service = Protocol::new(node_id.clone(), state, sender.clone());
    let reliable_sender_service = ReliableSend::new(protocol_service, sender);
    let timeout_layer =
        tower::timeout::TimeoutLayer::new(tokio::time::Duration::from_millis(delivery_time));
    let res = timeout_layer
        .layer(reliable_sender_service)
        .oneshot(MembershipMessage::new(node_id, None).into())
        .await;
    log::debug!("Membership sending result {:?}", res);
}
