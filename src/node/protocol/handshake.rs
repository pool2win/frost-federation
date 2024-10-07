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
use futures::{Future, FutureExt};
use serde::{Deserialize, Serialize};
use std::pin::Pin;
use std::task::{Context, Poll};
use tower::{BoxError, Service};

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Default)]
pub struct HandshakeMessage {
    pub sender_id: String,
    pub message: String,
    pub version: String,
}

impl HandshakeMessage {
    pub fn new(sender_id: String, message: String, version: String) -> Self {
        Self {
            sender_id,
            message,
            version,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Handshake {
    sender_id: String,
}

impl Handshake {
    pub fn new(node_id: String) -> Self {
        Handshake { sender_id: node_id }
    }
}

/// Service for handling Handshake protocol.
///
/// By making all protocol into a Service, we can use tower:Steer to
/// multiplex across services.
impl Service<Message> for Handshake {
    type Response = Option<Message>;
    type Error = BoxError;
    type Future = Pin<Box<dyn Future<Output = Result<Option<Message>, BoxError>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, msg: Message) -> Self::Future {
        let local_sender_id = self.sender_id.clone();
        async move {
            match msg {
                Message::Handshake(HandshakeMessage {
                    message,
                    sender_id,
                    version,
                }) => match message.as_str() {
                    "helo" => Ok(Some(Message::Handshake(HandshakeMessage {
                        message: "oleh".to_string(),
                        sender_id: local_sender_id,
                        version: "0.1.0".to_string(),
                    }))),
                    "oleh" => Ok(None),
                    _ => Ok(Some(Message::Handshake(HandshakeMessage {
                        message: "helo".to_string(),
                        sender_id: local_sender_id,
                        version: "0.1.0".to_string(),
                    }))),
                },
                _ => Ok(None),
            }
        }
        .boxed()
    }
}

#[cfg(test)]
mod handshake_tests {
    use crate::node::protocol::{Handshake, HandshakeMessage, Message};
    use tower::{Service, ServiceExt};

    #[tokio::test]
    async fn it_should_create_handshake_as_service_and_respond_to_default_message_with_handshake() {
        let mut p = Handshake {
            sender_id: "local".to_string(),
        };
        let res = p
            .ready()
            .await
            .unwrap()
            .call(HandshakeMessage::default().into())
            .await
            .unwrap();
        assert!(res.is_some());
        assert_eq!(
            res,
            Some(Message::Handshake(HandshakeMessage {
                message: "helo".to_string(),
                sender_id: "local".to_string(),
                version: "0.1.0".to_string()
            }))
        );
    }

    #[tokio::test]
    async fn it_should_create_handshake_as_service_and_respond_to_helo_with_oleh() {
        let mut p = Handshake {
            sender_id: "local".to_string(),
        };
        let res = p
            .ready()
            .await
            .unwrap()
            .call(Message::Handshake(HandshakeMessage {
                message: "helo".to_string(),
                sender_id: "local".to_string(),
                version: "0.1.0".to_string(),
            }))
            .await
            .unwrap();
        assert!(res.is_some());
        assert_eq!(
            res,
            Some(Message::Handshake(HandshakeMessage {
                message: "oleh".to_string(),
                sender_id: "local".to_string(),
                version: "0.1.0".to_string()
            }))
        );
    }

    #[tokio::test]
    async fn it_should_create_handshake_as_service_and_respond_to_oleh_with_none() {
        let mut p = Handshake {
            sender_id: "local".to_string(),
        };
        let res = p
            .ready()
            .await
            .unwrap()
            .call(Message::Handshake(HandshakeMessage {
                message: "oleh".to_string(),
                sender_id: "local".to_string(),
                version: "0.1.0".to_string(),
            }))
            .await
            .unwrap();
        assert!(res.is_none());
    }
}
