// Copyright 2024 Braidpool Developers

// This file is part of Braidpool

// Braidpool is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Braidpool is distributed in the hope that it will be useful, but
// WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU
// General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Braidpool. If not, see <https://www.gnu.org/licenses/>.

use super::{Message, ProtocolMessage};
use futures::{Future, FutureExt};
use serde::{Deserialize, Serialize};
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::SystemTime;
use tower::{BoxError, Service};

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct HeartbeatMessage {
    pub sender_id: String,
    pub time: SystemTime,
}

impl ProtocolMessage for HeartbeatMessage {
    fn start(node_id: &str) -> Option<Message> {
        Some(Message::Heartbeat(HeartbeatMessage {
            sender_id: node_id.to_string(),
            time: SystemTime::now(),
        }))
    }

    fn response_for_received(&self, _id: &str) -> Result<Option<Message>, String> {
        log::info!("Received {:?}", self);
        Ok(None)
    }
}

#[derive(Debug, Clone)]
pub struct Heartbeat {
    sender_id: String,
}

/// Service for handling Heartbeat protocol.
///
/// By making all protocol into a Service, we can use tower:Steer to
/// multiplex across services.
impl Service<Option<HeartbeatMessage>> for Heartbeat {
    type Response = Option<HeartbeatMessage>;
    type Error = BoxError;
    type Future = Pin<Box<dyn Future<Output = Result<Option<HeartbeatMessage>, BoxError>>>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, msg: Option<HeartbeatMessage>) -> Self::Future {
        let sender_id = self.sender_id.clone();
        async move {
            match msg {
                None => Ok(Some(HeartbeatMessage {
                    time: SystemTime::now(),
                    sender_id,
                })),
                Some(_) => Ok(None),
            }
        }
        .boxed()
    }
}

#[cfg(test)]
mod heartbeat_tests {

    use crate::node::protocol::{Heartbeat, HeartbeatMessage, Message, ProtocolMessage};
    use std::time::SystemTime;
    use tower::{Service, ServiceExt};

    #[test]
    fn it_matches_start_message_for_handshake() {
        if let Some(Message::Heartbeat(start_message)) = HeartbeatMessage::start("localhost".into())
        {
            assert!(start_message.time < SystemTime::now());
        }
    }

    #[test]
    fn it_matches_response_message_for_correct_handshake_start() {
        let start_message = HeartbeatMessage::start("localhost".into()).unwrap();
        let response = start_message.response_for_received("localhost").unwrap();
        assert_eq!(response, None);
    }

    #[tokio::test]
    async fn it_should_create_ping_as_service_and_respond_to_none_with_handshake() {
        let mut p = Heartbeat {
            sender_id: "local".to_string(),
        };
        let res = p.ready().await.unwrap().call(None).await.unwrap();
        assert!(res.is_some());
        assert_eq!(res.unwrap().sender_id, "local".to_string());
    }

    #[tokio::test]
    async fn it_should_create_ping_as_service_and_respond_to_some_with_none() {
        let mut p = Heartbeat {
            sender_id: "local".to_string(),
        };
        let res = p
            .ready()
            .await
            .unwrap()
            .call(Some(HeartbeatMessage {
                sender_id: "local".to_string(),
                time: SystemTime::now(),
            }))
            .await
            .unwrap();
        assert!(res.is_none());
    }
}
