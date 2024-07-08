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
use std::collections::HashMap;
use std::error::Error;
use tokio::sync::{mpsc, oneshot};

pub enum MembershipMessage {
    Add(String, ReliableSenderHandle, oneshot::Sender<()>),
    Remove(String, oneshot::Sender<Option<ReliableSenderHandle>>),
    GetSenders(oneshot::Sender<Vec<ReliableSenderHandle>>),
    Broadcast(Message, oneshot::Sender<()>),
}

#[derive(Debug)]
pub(crate) struct MembershipActor {
    members: HashMap<String, ReliableSenderHandle>,
    receiver: mpsc::Receiver<MembershipMessage>,
}

impl MembershipActor {
    pub fn start(receiver: mpsc::Receiver<MembershipMessage>) -> Self {
        Self {
            members: HashMap::default(),
            receiver,
        }
    }

    pub fn add_member(
        &mut self,
        addr: String,
        handle: ReliableSenderHandle,
        respond_to: oneshot::Sender<()>,
    ) {
        self.members.insert(addr, handle);
        let _ = respond_to.send(());
    }

    pub fn remove_member(
        &mut self,
        addr: String,
        respond_to: oneshot::Sender<Option<ReliableSenderHandle>>,
    ) {
        let removed = self.members.remove(&addr);
        let _ = respond_to.send(removed);
    }

    pub fn get_senders(&self, respond_to: oneshot::Sender<Vec<ReliableSenderHandle>>) {
        let m: Vec<ReliableSenderHandle> = self.members.values().cloned().collect();
        let _ = respond_to.send(m);
    }

    /// Send an Echo Broadcast message
    /// Use the reliable senders in the member's variable, then hand
    /// on further processing to EchoBroadcast
    pub async fn send_broadcast(&mut self, message: Message, respond_to: oneshot::Sender<()>) {
        for reliable_sender in self.members.values() {
            reliable_sender.send(message.clone()).await;
        }
    }
}

pub async fn run_membership_actor(mut actor: MembershipActor) {
    while let Some(message) = actor.receiver.recv().await {
        match message {
            MembershipMessage::Add(addr, handle, respond_to) => {
                actor.add_member(addr, handle, respond_to)
            }
            MembershipMessage::Remove(addr, respond_to) => actor.remove_member(addr, respond_to),
            MembershipMessage::Broadcast(message, respond_to) => {
                actor.send_broadcast(message, respond_to)
            }
            MembershipMessage::GetSenders(respond_to) => actor.get_senders(respond_to),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct MembershipHandle {
    sender: mpsc::Sender<MembershipMessage>,
}

impl MembershipHandle {
    pub async fn start() -> Self {
        let (sender, receiver) = mpsc::channel(32);
        let actor = MembershipActor::start(receiver);
        tokio::spawn(run_membership_actor(actor));
        Self { sender }
    }

    pub async fn add_member(
        &self,
        member: String,
        handle: ReliableSenderHandle,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let (respond_to, receiver) = oneshot::channel();
        let msg = MembershipMessage::Add(member, handle, respond_to);
        let _ = self.sender.send(msg).await;
        match receiver.await {
            Err(_) => Err("Error adding member".into()),
            Ok(_) => Ok(()),
        }
    }

    pub async fn remove_member(&self, member: String) -> Result<(), Box<dyn std::error::Error>> {
        let (respond_to, receiver) = oneshot::channel();
        let msg = MembershipMessage::Remove(member, respond_to);
        let _ = self.sender.send(msg).await;
        let response = receiver.await.unwrap();
        if response.is_some() {
            Ok(())
        } else {
            Err("Error removing member".into())
        }
    }

    pub async fn get_senders(&self) -> Result<Vec<ReliableSenderHandle>, Box<dyn Error>> {
        let (respond_to, receiver) = oneshot::channel();
        if self
            .sender
            .send(MembershipMessage::GetSenders(respond_to))
            .await
            .is_err()
        {
            return Err("Error sending request to get reliable senders".into());
        }
        match receiver.await {
            Err(_) => Err("Error reading membership".into()),
            Ok(result) => Ok(result),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn it_should_create_membership_add_and_remove_members() {
        let membership_handle = MembershipHandle::start().await;
        let reliable_sender_handle = ReliableSenderHandle::default();
        let reliable_sender_handle_2 = ReliableSenderHandle::default();

        let _ = membership_handle
            .add_member("localhost".to_string(), reliable_sender_handle)
            .await;

        let _ = membership_handle
            .add_member("localhost2".to_string(), reliable_sender_handle_2)
            .await;

        assert!(membership_handle
            .remove_member("localhost".to_string())
            .await
            .is_ok());
    }

    #[tokio::test]
    async fn it_should_result_in_error_when_removing_non_member() {
        let membership_handle = MembershipHandle::start().await;

        assert!(membership_handle
            .remove_member("localhost22".to_string())
            .await
            .is_err());
    }

    #[tokio::test]
    async fn it_should_return_members_as_empty_vec() {
        let membership_handle = MembershipHandle::start().await;

        let reliable_senders = membership_handle.get_senders().await;
        assert!(reliable_senders.unwrap().is_empty());
    }

    #[tokio::test]
    async fn it_should_return_members_as_vec() {
        let membership_handle = MembershipHandle::start().await;
        let mut reliable_sender_handle = ReliableSenderHandle::default();
        reliable_sender_handle
            .expect_clone()
            .returning(ReliableSenderHandle::default);
        let _ = membership_handle
            .add_member("localhost".to_string(), reliable_sender_handle)
            .await;

        let reliable_senders = membership_handle.get_senders().await;
        assert_eq!(reliable_senders.unwrap().len(), 1);
    }
}
