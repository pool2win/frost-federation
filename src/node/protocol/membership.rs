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
pub struct MembershipMessage {
    pub sender_id: String,
    pub message: Vec<String>,
}

impl MembershipMessage {
    pub fn new(sender_id: String, members: Vec<String>) -> Self {
        MembershipMessage {
            sender_id,
            message: members,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Membership {
    sender_id: String,
}

impl Membership {
    pub fn new(node_id: String) -> Self {
        Membership { sender_id: node_id }
    }
}

/// Service for handling Membership protocol.
impl Service<Message> for Membership {
    type Response = Option<Message>;
    type Error = BoxError;
    type Future = Pin<Box<dyn Future<Output = Result<Option<Message>, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    /// Membership doesn't respond with anything. It is pushed by a
    /// node when anyone connects to it.
    fn call(&mut self, _msg: Message) -> Self::Future {
        async move { Ok(None) }.boxed()
    }
}

#[cfg(test)]
mod membership_tests {

    use super::Membership;
    use crate::node::protocol::MembershipMessage;
    use tower::{Service, ServiceExt};

    #[tokio::test]
    async fn it_should_create_membership_as_service_and_respond_to_none_with_membership() {
        let mut p = Membership::new("local".to_string());
        let res = p
            .ready()
            .await
            .unwrap()
            .call(MembershipMessage::default().into())
            .await
            .unwrap();
        assert!(res.is_none());
        // assert_eq!(
        //     res,
        //     Some(MembershipMessage::new("local".to_string(), vec!["a".to_string()]).into())
        // );
    }

    #[tokio::test]
    async fn it_should_create_membership_as_service_and_respond_to_membership_with_none() {
        let mut p = Membership::new("local".to_string());
        let res = p
            .ready()
            .await
            .unwrap()
            .call(MembershipMessage::new("local".to_string(), vec!["a".to_string()]).into())
            .await
            .unwrap();
        assert!(res.is_none());
    }

    #[test]
    fn it_should_create_default_membership_message() {
        assert_eq!(MembershipMessage::default().sender_id, "".to_string())
    }
}
