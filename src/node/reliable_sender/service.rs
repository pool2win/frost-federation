use crate::node::protocol::Message;
#[mockall_double::double]
use crate::node::reliable_sender::ReliableSenderHandle;
use futures::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use tower::{BoxError, Service};

/// Service for sending protocol messages using reliable sender
#[derive(Debug, Clone)]
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
    S: Service<Message, Response = Option<Message>> + Send + Clone + 'static,
    S::Error: Into<BoxError>,
{
    type Response = ();
    type Error = BoxError;
    type Future = Pin<Box<dyn Future<Output = Result<(), BoxError>>>>;

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
                    let _x = this.sender.send(msg).await;
                    Ok(())
                }
                _ => Ok(()),
                //Err(e) => Err(Box::new(e)),
            }
        })
    }
}
