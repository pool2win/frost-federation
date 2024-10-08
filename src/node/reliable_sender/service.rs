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
#[mockall_double::double]
use crate::node::reliable_sender::ReliableSenderHandle;
use futures::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use tower::{BoxError, Service};

/// Service for sending protocol messages using reliable sender
#[derive(Clone)]
pub struct ReliableSend<S> {
    inner: S,
    sender: ReliableSenderHandle,
}

impl<S> ReliableSend<S> {
    pub fn new(svc: S, sender: ReliableSenderHandle) -> Self {
        ReliableSend { inner: svc, sender }
    }
}

impl<S> Service<Message> for ReliableSend<S>
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
                    if this.sender.send(msg).await.is_ok() {
                        Ok(())
                    } else {
                        Err("Error sending reliable message".into())
                    }
                }
                _ => Err("Unknown message type in reliable sender".into()),
            }
        })
    }
}
