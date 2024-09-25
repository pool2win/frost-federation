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

use crate::node::protocol::Message;
use futures::Future;
use serde::{Deserialize, Serialize};
use std::pin::Pin;
use std::task::{Context, Poll};
use tower::{BoxError, Service};

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct PingMessage {
    pub sender_id: String,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct Ping {
    sender_id: String,
}

impl Ping {
    pub fn new(node_id: String) -> Self {
        Ping { sender_id: node_id }
    }
}

/// Service for handling Ping protocol.
///
/// By making all protocol into a Service, we can use tower:Steer to
/// multiplex across services.
impl Service<Option<Message>> for Ping {
    type Response = Option<Message>;
    type Error = BoxError;
    type Future = Pin<Box<dyn Future<Output = Result<Option<Message>, BoxError>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, msg: Option<Message>) -> Self::Future {
        let local_sender_id = self.sender_id.clone();
        Box::pin(async move {
            match msg {
                Some(Message::Ping(PingMessage { message, sender_id })) => match message.as_str() {
                    "ping" => Ok(Some(Message::Ping(PingMessage {
                        message: "pong".to_string(),
                        sender_id: local_sender_id,
                    }))),
                    _ => Ok(None),
                },
                _ => Ok(Some(Message::Ping(PingMessage {
                    message: "ping".to_string(),
                    sender_id: local_sender_id,
                }))),
            }
        })
    }
}

#[cfg(test)]
mod ping_tests {

    use super::Ping;
    use crate::node::protocol::{Message, PingMessage};
    use tower::{Service, ServiceExt};

    #[tokio::test]
    async fn it_should_create_ping_as_service_and_respond_to_none_with_ping() {
        let mut p = Ping {
            sender_id: "local".to_string(),
        };
        let res = p.ready().await.unwrap().call(None).await.unwrap();
        assert!(res.is_some());
        assert_eq!(
            res,
            Some(Message::Ping(PingMessage {
                message: "ping".to_string(),
                sender_id: "local".to_string()
            }))
        );
    }

    #[tokio::test]
    async fn it_should_create_ping_as_service_and_respond_to_ping_with_pong() {
        let mut p = Ping {
            sender_id: "local".to_string(),
        };
        let res = p
            .ready()
            .await
            .unwrap()
            .call(Some(Message::Ping(PingMessage {
                message: "ping".to_string(),
                sender_id: "local".to_string(),
            })))
            .await
            .unwrap();
        assert!(res.is_some());
        assert_eq!(
            res,
            Some(Message::Ping(PingMessage {
                message: "pong".to_string(),
                sender_id: "local".to_string()
            }))
        );
    }

    #[tokio::test]
    async fn it_should_create_ping_as_service_and_respond_to_pong_with_none() {
        let mut p = Ping {
            sender_id: "local".to_string(),
        };
        let res = p
            .ready()
            .await
            .unwrap()
            .call(Some(Message::Ping(PingMessage {
                message: "pong".to_string(),
                sender_id: "local".to_string(),
            })))
            .await
            .unwrap();
        assert!(res.is_none());
    }
}
