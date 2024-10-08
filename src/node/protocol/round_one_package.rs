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

use super::Broadcast;

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Default)]
pub struct RoundOnePackageMessage {
    pub sender_id: String,
    pub message: String,
}

impl RoundOnePackageMessage {
    pub fn new(sender_id: String, message: String) -> Self {
        RoundOnePackageMessage { sender_id, message }
    }
}

#[derive(Debug, Clone, Default)]
pub struct RoundOnePackage {
    sender_id: String,
}

impl RoundOnePackage {
    pub fn new(node_id: String) -> Self {
        RoundOnePackage { sender_id: node_id }
    }
}

/// Service for handling RoundOnePackage protocol.
///
/// By making all protocol into a Service, we can use tower:Steer to
/// multiplex across services.
impl Service<Message> for RoundOnePackage {
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
                Message::BroadcastMessage(Broadcast::RoundOnePackage(m)) => {
                    match m.message.as_str() {
                        "" => Ok(Some(
                            RoundOnePackageMessage::new(
                                local_sender_id,
                                "round_one_package".to_string(),
                            )
                            .into(),
                        )),
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
mod round_one_package_tests {

    use super::RoundOnePackage;
    use crate::node::protocol::RoundOnePackageMessage;
    use tower::{Service, ServiceExt};

    #[tokio::test]
    async fn it_should_create_round_one_package_as_service_and_respond_to_none_with_round_one_package(
    ) {
        let mut p = RoundOnePackage::new("local".to_string());
        let res = p
            .ready()
            .await
            .unwrap()
            .call(RoundOnePackageMessage::default().into())
            .await
            .unwrap();
        assert!(res.is_some());
        assert_eq!(
            res,
            Some(
                RoundOnePackageMessage::new("local".to_string(), "round_one_package".to_string())
                    .into()
            )
        );
    }

    #[tokio::test]
    async fn it_should_create_round_one_package_as_service_and_respond_to_round_one_package_with_none(
    ) {
        let mut p = RoundOnePackage::new("local".to_string());
        let res = p
            .ready()
            .await
            .unwrap()
            .call(
                RoundOnePackageMessage::new("local".to_string(), "round_one_package".to_string())
                    .into(),
            )
            .await
            .unwrap();
        assert!(res.is_none());
    }

    #[test]
    fn it_should_create_default_round_one_package_message() {
        assert_eq!(RoundOnePackageMessage::default().sender_id, "".to_string())
    }
}
