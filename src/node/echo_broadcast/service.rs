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

use crate::node::echo_broadcast::EchoBroadcastHandle;
use crate::node::membership::ReliableSenderMap;
use crate::node::protocol::Message;
use futures::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use tower::{BoxError, Service};

/// Service for sending protocol messages using echo broadcast
#[derive(Clone)]
pub struct EchoBroadcast<S> {
    inner: S,
    handle: EchoBroadcastHandle,
}

impl<S> EchoBroadcast<S> {
    pub fn new(svc: S, handle: EchoBroadcastHandle) -> Self {
        EchoBroadcast { inner: svc, handle }
    }
}

impl<S> Service<Message> for EchoBroadcast<S>
where
    S: Service<Message, Response = Option<Message>> + Clone + Send + 'static,
    S::Error: Into<BoxError> + Send,
    S::Future: Send,
{
    type Response = ();
    type Error = BoxError;
    type Future = Pin<Box<dyn Future<Output = Result<(), BoxError>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), BoxError>> {
        match self.inner.poll_ready(cx) {
            Poll::Ready(_) => Poll::Ready(Ok(())),
            _ => Poll::Pending,
        }
    }

    fn call(&mut self, msg: Message) -> Self::Future {
        let mut this = self.clone();

        Box::pin(async move {
            let response_message = this.inner.call(msg).await;
            match response_message {
                Ok(Some(msg)) => {
                    // TODO: Get this membership from Node::state
                    let members = ReliableSenderMap::new();
                    if this.handle.send(msg, members).await.is_ok() {
                        Ok(())
                    } else {
                        Err("Error sending echo broadcast".into())
                    }
                }
                _ => Err("Unknown message type in echo broadcast handle".into()),
            }
        })
    }
}

#[cfg(test)]
mod echo_broadcast_service_tests {
    use futures::FutureExt;
    use std::collections::HashMap;
    use std::time::SystemTime;

    use super::*;
    use crate::node::protocol::message_id_generator::MessageIdGenerator;
    use crate::node::protocol::HeartbeatMessage;
    #[mockall_double::double]
    use crate::node::reliable_sender::ReliableSenderHandle;

    #[tokio::test]
    async fn it_should_run_service_without_error() {
        // ReliableSenderHandle::default();
        // let mut mock_reliable_sender = ReliableSenderHandle::default();
        // mock_reliable_sender
        //     .expect_send()
        //     .return_once(|_| async { Ok(()) }.boxed());
        // let reliable_senders_map = HashMap::from([("a".to_string(), mock_reliable_sender)]);
        // let message_id_generator = MessageIdGenerator::new("localhost".to_string());

        // let handle = EchoBroadcastHandle::start(message_id_generator, reliable_senders_map);

        // let message = Message::Heartbeat(HeartbeatMessage {
        //     sender_id: "localhost".into(),
        //     time: SystemTime::now(),
        // });

        // let result = handle.send(message).await;
        // assert!(result.is_err());
    }
}
