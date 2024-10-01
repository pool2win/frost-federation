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

use super::Message;
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

impl Default for HeartbeatMessage {
    fn default() -> Self {
        HeartbeatMessage {
            sender_id: String::default(),
            time: SystemTime::now(),
        }
    }
}

impl HeartbeatMessage {
    pub fn default_as_message() -> Message {
        Message::Heartbeat(HeartbeatMessage::default())
    }

    pub fn new(sender_id: String, time: SystemTime) -> Message {
        Message::Heartbeat(HeartbeatMessage { sender_id, time })
    }
}

#[derive(Debug, Clone)]
pub struct Heartbeat {
    sender_id: String,
}

impl Heartbeat {
    pub fn new(node_id: String) -> Self {
        Heartbeat { sender_id: node_id }
    }
}

/// Service for handling Heartbeat protocol.
///
/// By making all protocol into a Service, we can use tower:Steer to
/// multiplex across services.
impl Service<Message> for Heartbeat {
    type Response = Option<Message>;
    type Error = BoxError;
    type Future = Pin<Box<dyn Future<Output = Result<Option<Message>, BoxError>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, msg: Message) -> Self::Future {
        let self_sender_id = self.sender_id.clone();
        async move {
            match msg {
                Message::Heartbeat(HeartbeatMessage { sender_id, time }) => {
                    match sender_id.as_str() {
                        "" => Ok(Some(Message::Heartbeat(HeartbeatMessage {
                            time: SystemTime::now(),
                            sender_id: self_sender_id,
                        }))),
                        _ => Ok(None),
                    }
                }
                _ => Ok(None),
            }
        }
        .boxed()
    }
}

#[cfg(test)]
mod heartbeat_tests {

    use crate::node::protocol::{Heartbeat, HeartbeatMessage, Message};
    use std::time::SystemTime;
    use tower::{Service, ServiceExt};

    #[tokio::test]
    async fn it_should_create_heartbeat_as_service_and_respond_to_heartbeat_with_none() {
        let mut p = Heartbeat {
            sender_id: "local".to_string(),
        };
        let res = p
            .ready()
            .await
            .unwrap()
            .call(HeartbeatMessage::new(
                "local".to_string(),
                SystemTime::now(),
            ))
            .await
            .unwrap();
        assert!(res.is_none());
    }

    #[tokio::test]
    async fn it_should_create_heartbeat_as_service_and_respond_to_default_with_heartbeat() {
        let mut p = Heartbeat {
            sender_id: "local".to_string(),
        };
        if let Some(Message::Heartbeat(msg)) = p
            .ready()
            .await
            .unwrap()
            .call(HeartbeatMessage::default_as_message())
            .await
            .unwrap()
        {
            assert_eq!(msg.sender_id, "local".to_string());
        } else {
            assert!(false, "Message not a heartbeat message");
        }
    }

    #[tokio::test]
    async fn it_should_create_heartbeat_as_service_and_respond_to_some_with_none() {
        let mut p = Heartbeat {
            sender_id: "local".to_string(),
        };
        let res = p
            .ready()
            .await
            .unwrap()
            .call(Message::Heartbeat(HeartbeatMessage {
                sender_id: "local".to_string(),
                time: SystemTime::now(),
            }))
            .await
            .unwrap();
        assert!(res.is_none());
    }
}
