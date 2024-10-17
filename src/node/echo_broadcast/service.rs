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
    node_id: String,
    handle: EchoBroadcastHandle,
}

impl<S> EchoBroadcast<S> {
    pub fn new(svc: S, handle: EchoBroadcastHandle, state: State, node_id: String) -> Self {
        EchoBroadcast {
            inner: svc,
            state,
            node_id,
            handle,
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
            let members = this.state.membership_handle.get_members().await.unwrap();
            log::debug!("Handling message {:?} in service ...", msg);
            match msg.clone() {
                Message::Unicast(_) => {}
                Message::Echo(..) => {}
                // mid is available in Broadcast - implies we received this message
                Message::Broadcast(m, Some(mid)) => {
                    log::debug!("Generating echo...");
                    let echo_msg = Message::Echo(m.clone(), mid.clone(), this.node_id.clone());
                    log::debug!("Sending ECHO ... {:?}", echo_msg);
                    this.handle.send_echo(echo_msg, members.clone()).await?;

                    log::debug!("ECHO Sent ...");
                    this.handle
                        .track_received_broadcast(
                            Message::Broadcast(m, Some(mid)),
                            this.node_id.clone(),
                            members.clone(),
                        )
                        .await?;
                    log::info!("Deliver ECHO ...");
                }
                // mid is not available in Broadcast - implies we are sending this broadcast
                Message::Broadcast(m, None) => {
                    log::debug!("Generating message_id for Send...");
                    let to_send =
                        Message::Broadcast(m, Some(this.state.message_id_generator.next()));
                    this.handle.send(to_send, members.clone()).await?;
                }
            };
            let response_message = this.inner.call(msg).await;
            match response_message {
                Ok(Some(msg)) => {
                    log::debug!("Sending echo broadcast with message {:?}", msg);
                    if this.handle.send(msg, members).await.is_ok() {
                        Ok(())
                    } else {
                        Err("Error sending echo broadcast".into())
                    }
                }
                _ => {
                    log::info!("No response to send for received broadcast");
                    Ok(())
                }
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
        mock_reliable_sender.expect_clone().returning(|| {
            let mut mock = ReliableSenderHandle::default();
            mock.expect_clone().returning(ReliableSenderHandle::default);
            mock.expect_send().return_once(|_| async { Ok(()) }.boxed());
            mock
        });
        let membership_handle = MembershipHandle::start("localhost".to_string()).await;
        let _ = membership_handle
            .add_member("a".to_string(), mock_reliable_sender.clone())
            .await;
        let message_id_generator = MessageIdGenerator::new("localhost".to_string());
        let state = State::new(membership_handle, message_id_generator);
        let mut echo_bcast_handle = EchoBroadcastHandle::default();
        echo_bcast_handle.expect_clone().returning(|| {
            let mut mocked = EchoBroadcastHandle::default();
            mocked.expect_send_echo().returning(|_, _| Ok(()));
            mocked
        });
        let message = HeartbeatMessage {
            sender_id: "localhost".into(),
            time: SystemTime::now(),
        }
        .into();

        let handshake_service =
            Protocol::new("localhost".to_string(), state.clone(), mock_reliable_sender);
        let echo_broadcast_service = EchoBroadcast::new(
            handshake_service,
            echo_bcast_handle,
            state,
            "localhost".into(),
        );
        let result = echo_broadcast_service.oneshot(message).await;
        assert!(result.is_ok());
    }
}
