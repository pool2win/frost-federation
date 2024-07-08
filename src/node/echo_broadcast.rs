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

use crate::node::membership::MembershipHandle;
use crate::node::protocol::Message;
use std::error::Error;
use tokio::sync::mpsc;

/// Message types for echo broadcast actor -> handle communication
pub(crate) enum EchoBroadcastMessage {
    Send(Message),
    Echo(Message),
}

/// Echo Broadcast Actor model implementation.
/// The actor receives messages from EchoBroadcastHandle
pub(crate) struct EchoBroadcastActor {
    receiver: mpsc::Receiver<EchoBroadcastMessage>,
    membership_handle: MembershipHandle,
}

impl EchoBroadcastActor {
    pub fn start(
        receiver: mpsc::Receiver<EchoBroadcastMessage>,
        membership_handle: MembershipHandle,
    ) -> Self {
        Self {
            receiver,
            membership_handle,
        }
    }

    pub async fn handle_message(&mut self, message: EchoBroadcastMessage) {
        match message {
            EchoBroadcastMessage::Send(message) => {
                // TODO: Send using reliable sender
                self.send_message(message).await;
                // TODO: Book keeping for waiting for echo
            }
            EchoBroadcastMessage::Echo(message) => {
                // TODO: Update waiting for echo
                self.handle_received_echo(message).await;
            }
        }
    }

    /// Send message using the reliable sender
    /// Return the result from the reliable sender's send method
    pub async fn send_message(
        &mut self,
        message: Message,
    ) -> Result<(), Box<dyn Error + Sync + Send>> {
        //self.reliable_sender.send(message).await
        Ok(())
    }

    pub async fn handle_received_echo(&mut self, message: Message) {}
}

/// Handler for sending and confirming echo messages for a broadcast
/// Send using ReliableSenderHandle and then wait fro Echo message
#[derive(Clone, Debug)]
pub(crate) struct EchoBroadcastHandle {
    sender: mpsc::Sender<EchoBroadcastMessage>,
    delivery_timeout: u64,
}

async fn start_echo_broadcast(mut actor: EchoBroadcastActor) {
    while let Some(message) = actor.receiver.recv().await {
        actor.handle_message(message).await;
    }
}

impl EchoBroadcastHandle {
    pub fn start(delivery_timeout: u64, membership_handle: MembershipHandle) -> Self {
        let (sender, receiver) = mpsc::channel(32);
        let actor = EchoBroadcastActor::start(receiver, membership_handle);
        tokio::spawn(start_echo_broadcast(actor));

        Self {
            sender,
            delivery_timeout,
        }
    }

    pub async fn send_broadcast(&self, message: Message) -> Result<(), Box<dyn Error>> {
        let msg = EchoBroadcastMessage::Send(message);
        if self.sender.send(msg).await.is_err() {
            Err("Error sending broadcast message".into())
        } else {
            Ok(())
        }
    }
}
