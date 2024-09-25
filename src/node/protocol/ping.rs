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

use super::{Message, ProtocolMessage};
use futures::{Future, FutureExt};
use serde::{Deserialize, Serialize};
use std::pin::Pin;
use std::task::{Context, Poll};
use tower::{BoxError, Service};

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct PingMessage {
    pub sender_id: String,
    pub message: String,
}

impl ProtocolMessage for PingMessage {
    fn start(node_id: &str) -> Option<Message> {
        Some(Message::Ping(PingMessage {
            sender_id: node_id.to_string(),
            message: String::from("ping"),
        }))
    }

    fn response_for_received(&self, id: &str) -> Result<Option<Message>, String> {
        if self.message == "ping" {
            Ok(Some(Message::Ping(PingMessage {
                sender_id: id.to_string(),
                message: String::from("pong"),
            })))
        } else {
            Ok(None)
        }
    }
}

#[derive(Debug, Clone)]
pub struct Ping {
    sender_id: String,
}

/// Service for handling Ping protocol.
///
/// By making all protocol into a Service, we can use tower:Steer to
/// multiplex across services.
impl Service<Option<PingMessage>> for Ping {
    type Response = Option<PingMessage>;
    type Error = BoxError;
    type Future = Pin<Box<dyn Future<Output = Result<Option<PingMessage>, BoxError>>>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, msg: Option<PingMessage>) -> Self::Future {
        let sender_id = self.sender_id.clone();
        async move {
            match msg {
                None => Ok(Some(PingMessage {
                    message: "ping".to_string(),
                    sender_id,
                })),
                Some(msg) => match msg.message.as_str() {
                    "ping" => Ok(Some(PingMessage {
                        message: "pong".to_string(),
                        sender_id,
                    })),
                    _ => Ok(None),
                },
            }
        }
        .boxed()
    }
}

#[cfg(test)]
mod ping_tests {

    use super::Ping;
    use crate::node::protocol::{Message, PingMessage, ProtocolMessage};
    use tower::{Service, ServiceExt};

    #[test]
    fn it_matches_start_message_for_ping() {
        if let Some(Message::Ping(start_message)) = PingMessage::start("localhost".into()) {
            assert_eq!(start_message.message, "ping".to_string());
        }
    }

    #[test]
    fn it_matches_response_message_for_correct_handshake_start() {
        let start_message = PingMessage::start("localhost".into()).unwrap();
        let response = start_message
            .response_for_received("localhost")
            .unwrap()
            .unwrap();
        assert_eq!(
            response,
            Message::Ping(PingMessage {
                sender_id: "localhost".to_string(),
                message: "pong".to_string()
            })
        );
    }

    #[tokio::test]
    async fn it_should_create_ping_as_service_and_respond_to_none_with_ping() {
        let mut p = Ping {
            sender_id: "local".to_string(),
        };
        let res = p.ready().await.unwrap().call(None).await.unwrap();
        assert!(res.is_some());
        assert_eq!(
            res,
            Some(PingMessage {
                message: "ping".to_string(),
                sender_id: "local".to_string()
            })
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
            .call(Some(PingMessage {
                message: "ping".to_string(),
                sender_id: "local".to_string(),
            }))
            .await
            .unwrap();
        assert!(res.is_some());
        assert_eq!(
            res,
            Some(PingMessage {
                message: "pong".to_string(),
                sender_id: "local".to_string()
            })
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
            .call(Some(PingMessage {
                message: "pong".to_string(),
                sender_id: "local".to_string(),
            }))
            .await
            .unwrap();
        assert!(res.is_none());
    }
}
