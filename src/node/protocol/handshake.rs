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
pub struct HandshakeMessage {
    pub sender_id: String,
    pub message: String,
    pub version: String,
}

impl ProtocolMessage for HandshakeMessage {
    fn start(node_id: &str) -> Option<Message> {
        Some(Message::Handshake(HandshakeMessage {
            sender_id: node_id.to_string(),
            message: String::from("helo"),
            version: String::from("0.1.0"),
        }))
    }

    fn response_for_received(&self, id: &str) -> Result<Option<Message>, String> {
        log::info!("Received {:?}", self);
        match self {
            HandshakeMessage {
                sender_id: _,
                message,
                version,
            } if message == "helo" && version == "0.1.0" => {
                Ok(Some(Message::Handshake(HandshakeMessage {
                    sender_id: id.to_string(),
                    message: String::from("oleh"),
                    version: String::from("0.1.0"),
                })))
            }
            HandshakeMessage {
                sender_id: _,
                message,
                version,
            } if message == "oleh" && version == "0.1.0" => Ok(None),
            _ => Err("Bad message".to_string()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Handshake {
    sender_id: String,
}

/// Service for handling Handshake protocol.
///
/// By making all protocol into a Service, we can use tower:Steer to
/// multiplex across services.
impl Service<Option<HandshakeMessage>> for Handshake {
    type Response = Option<HandshakeMessage>;
    type Error = BoxError;
    type Future = Pin<Box<dyn Future<Output = Result<Option<HandshakeMessage>, BoxError>>>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, msg: Option<HandshakeMessage>) -> Self::Future {
        let sender_id = self.sender_id.clone();
        async move {
            match msg {
                None => Ok(Some(HandshakeMessage {
                    message: "helo".to_string(),
                    sender_id,
                    version: "0.1.0".to_string(),
                })),
                Some(msg) => match msg.message.as_str() {
                    "helo" => Ok(Some(HandshakeMessage {
                        message: "oleh".to_string(),
                        sender_id,
                        version: "0.1.0".to_string(),
                    })),
                    _ => Ok(None),
                },
            }
        }
        .boxed()
    }
}

#[cfg(test)]
mod handshake_tests {
    use crate::node::protocol::{Handshake, HandshakeMessage, Message, ProtocolMessage};
    use tower::{Service, ServiceExt};

    #[test]
    fn it_matches_start_message_for_handshake() {
        let start_message = HandshakeMessage::start("localhost".into()).unwrap();
        assert_eq!(
            start_message,
            Message::Handshake(HandshakeMessage {
                sender_id: "localhost".into(),
                message: String::from("helo"),
                version: String::from("0.1.0"),
            })
        );
    }

    #[test]
    fn it_matches_response_message_for_correct_handshake_start() {
        let start_message = HandshakeMessage::start("localhost".into()).unwrap();
        let response = start_message.response_for_received("localhost").unwrap();
        assert_eq!(
            response,
            Some(Message::Handshake(HandshakeMessage {
                sender_id: "localhost".into(),
                message: String::from("oleh"),
                version: String::from("0.1.0"),
            }))
        );
    }

    #[test]
    fn it_matches_error_response_message_for_incorrect_handshake_start() {
        let start_message = Message::Handshake(HandshakeMessage {
            sender_id: "localhost".into(),
            message: String::from("bad-message"),
            version: String::from("0.1.0"),
        });

        let response = start_message.response_for_received("localhost");
        assert_eq!(response, Err("Bad message".to_string()));
    }

    #[test]
    fn it_matches_error_response_message_for_incorrect_handshake_version() {
        let start_message = Message::Handshake(HandshakeMessage {
            sender_id: "localhost".into(),
            message: String::from("helo"),
            version: String::from("0.2.0"),
        });

        let response = start_message.response_for_received("localhost");
        assert_eq!(response, Err("Bad message".to_string()));
    }

    #[tokio::test]
    async fn it_should_create_ping_as_service_and_respond_to_none_with_handshake() {
        let mut p = Handshake {
            sender_id: "local".to_string(),
        };
        let res = p.ready().await.unwrap().call(None).await.unwrap();
        assert!(res.is_some());
        assert_eq!(
            res,
            Some(HandshakeMessage {
                message: "helo".to_string(),
                sender_id: "local".to_string(),
                version: "0.1.0".to_string()
            })
        );
    }

    #[tokio::test]
    async fn it_should_create_ping_as_service_and_respond_to_helo_with_oleh() {
        let mut p = Handshake {
            sender_id: "local".to_string(),
        };
        let res = p
            .ready()
            .await
            .unwrap()
            .call(Some(HandshakeMessage {
                message: "helo".to_string(),
                sender_id: "local".to_string(),
                version: "0.1.0".to_string(),
            }))
            .await
            .unwrap();
        assert!(res.is_some());
        assert_eq!(
            res,
            Some(HandshakeMessage {
                message: "oleh".to_string(),
                sender_id: "local".to_string(),
                version: "0.1.0".to_string()
            })
        );
    }

    #[tokio::test]
    async fn it_should_create_ping_as_service_and_respond_to_oleh_with_none() {
        let mut p = Handshake {
            sender_id: "local".to_string(),
        };
        let res = p
            .ready()
            .await
            .unwrap()
            .call(Some(HandshakeMessage {
                message: "oleh".to_string(),
                sender_id: "local".to_string(),
                version: "0.1.0".to_string(),
            }))
            .await
            .unwrap();
        assert!(res.is_none());
    }
}
