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

use crate::node::protocol::{Message, Unicast};
use futures::{Future, FutureExt};
use serde::{Deserialize, Serialize};
use std::pin::Pin;
use std::task::{Context, Poll};
use tower::{BoxError, Service};

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Default)]
pub struct PingMessage {
    pub sender_id: String,
    pub message: String,
}

impl PingMessage {
    pub fn new(sender_id: String, message: String) -> Self {
        PingMessage { sender_id, message }
    }
}

#[derive(Debug, Clone, Default)]
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
impl Service<Message> for Ping {
    type Response = Option<Message>;
    type Error = BoxError;
    type Future = Pin<Box<dyn Future<Output = Result<Option<Message>, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, msg: Message) -> Self::Future {
        let local_sender_id = self.sender_id.clone();
        async move {
            match msg {
                Message::Unicast(Unicast::Ping(m)) => match m.message.as_str() {
                    "" => Ok(Some(
                        PingMessage::new(local_sender_id, "ping".to_string()).into(),
                    )),
                    "ping" => Ok(Some(
                        PingMessage::new(local_sender_id, "pong".to_string()).into(),
                    )),
                    _ => Ok(None),
                },
                _ => Ok(None),
            }
        }
        .boxed()
    }
}

#[cfg(test)]
mod ping_tests {

    use super::Ping;
    use crate::node::protocol::PingMessage;
    use tower::{Service, ServiceExt};

    #[tokio::test]
    async fn it_should_create_ping_as_service_and_respond_to_none_with_ping() {
        let mut p = Ping::new("local".to_string());
        let res = p
            .ready()
            .await
            .unwrap()
            .call(PingMessage::default().into())
            .await
            .unwrap();
        assert!(res.is_some());
        assert_eq!(
            res,
            Some(PingMessage::new("local".to_string(), "ping".to_string()).into())
        );
    }

    #[tokio::test]
    async fn it_should_create_ping_as_service_and_respond_to_ping_with_pong() {
        let mut p = Ping::new("local".to_string());
        let res = p
            .ready()
            .await
            .unwrap()
            .call(PingMessage::new("local".to_string(), "ping".to_string()).into())
            .await
            .unwrap();
        assert!(res.is_some());
        assert_eq!(
            res,
            Some(PingMessage::new("local".to_string(), "pong".to_string()).into())
        );
    }

    #[tokio::test]
    async fn it_should_create_ping_as_service_and_respond_to_pong_with_none() {
        let mut p = Ping::new("local".to_string());
        let res = p
            .ready()
            .await
            .unwrap()
            .call(PingMessage::new("local".to_string(), "pong".to_string()).into())
            .await
            .unwrap();
        assert!(res.is_none());
    }

    #[test]
    fn it_should_create_default_ping_message() {
        assert_eq!(PingMessage::default().sender_id, "".to_string())
    }
}
