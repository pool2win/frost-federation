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

#[mockall_double::double]
use crate::node::echo_broadcast::EchoBroadcastHandle;
use crate::node::protocol::Message;
use crate::node::state::State;
use futures::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use tower::{BoxError, Service};

/// Service for sending protocol messages using echo broadcast
#[derive(Clone)]
pub struct EchoBroadcast<S> {
    inner: S,
    state: State,
    handle: EchoBroadcastHandle,
}

impl<S> EchoBroadcast<S> {
    pub fn new(svc: S, handle: EchoBroadcastHandle, state: State) -> Self {
        EchoBroadcast {
            inner: svc,
            handle,
            state,
        }
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
                    let members = this.state.membership_handle.get_members().await.unwrap();
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
    use std::time::SystemTime;
    use tower::ServiceExt;

    use super::*;
    #[mockall_double::double]
    use crate::node::echo_broadcast::EchoBroadcastHandle;
    use crate::node::membership::MembershipHandle;
    use crate::node::protocol::message_id_generator::MessageIdGenerator;
    use crate::node::protocol::{HeartbeatMessage, Protocol};
    #[mockall_double::double]
    use crate::node::reliable_sender::ReliableSenderHandle;

    #[tokio::test]
    async fn it_should_run_service_without_error() {
        ReliableSenderHandle::default();
        let mut mock_reliable_sender = ReliableSenderHandle::default();
        mock_reliable_sender
            .expect_send()
            .return_once(|_| async { Ok(()) }.boxed());
        let membership_handle = MembershipHandle::start("localhost".to_string()).await;
        let _ = membership_handle
            .add_member("a".to_string(), mock_reliable_sender)
            .await;
        let message_id_generator = MessageIdGenerator::new("localhost".to_string());
        let state = State::new(membership_handle, message_id_generator);
        let mut echo_bcast_handle = EchoBroadcastHandle::default();
        echo_bcast_handle
            .expect_clone()
            .returning(EchoBroadcastHandle::default);
        let message = HeartbeatMessage {
            sender_id: "localhost".into(),
            time: SystemTime::now(),
        }
        .into();

        let handshake_service = Protocol::new("localhost".to_string(), state.clone());
        let echo_broadcast_service =
            EchoBroadcast::new(handshake_service, echo_bcast_handle, state);
        let result = echo_broadcast_service.oneshot(message).await;
        assert!(result.is_err());
    }
}
