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
use crate::node::reliable_sender::ReliableSenderHandle;
use std::collections::HashMap;
use std::error::Error;
use tokio::sync::{mpsc, oneshot};

pub type ReliableSenderMap = HashMap<String, ReliableSenderHandle>;

pub enum MembershipMessage {
    Add(String, ReliableSenderHandle, oneshot::Sender<()>),
    Remove(String, oneshot::Sender<Option<ReliableSenderHandle>>),
    GetMembers(oneshot::Sender<ReliableSenderMap>),
}

#[derive(Debug)]
pub(crate) struct MembershipActor {
    members: ReliableSenderMap,
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
        node_id: String,
        handle: ReliableSenderHandle,
        respond_to: oneshot::Sender<()>,
    ) {
        self.members.insert(node_id, handle);
        let _ = respond_to.send(());
    }

    pub fn remove_member(
        &mut self,
        node_id: String,
        respond_to: oneshot::Sender<Option<ReliableSenderHandle>>,
    ) {
        let removed = self.members.remove(&node_id);
        let _ = respond_to.send(removed);
    }

    pub fn get_members(&self, respond_to: oneshot::Sender<ReliableSenderMap>) {
        let _ = respond_to.send(self.members.clone());
    }
}

pub async fn run_membership_actor(mut actor: MembershipActor) {
    while let Some(message) = actor.receiver.recv().await {
        match message {
            MembershipMessage::Add(node_id, handle, respond_to) => {
                actor.add_member(node_id, handle, respond_to)
            }
            MembershipMessage::Remove(node_id, respond_to) => {
                actor.remove_member(node_id, respond_to)
            }
            MembershipMessage::GetMembers(respond_to) => actor.get_members(respond_to),
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct MembershipHandle {
    sender: mpsc::Sender<MembershipMessage>,
}

impl MembershipHandle {
    pub async fn start(node_id: String) -> Self {
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
        let msg = MembershipMessage::Add(member.clone(), handle, respond_to);
        let _ = self.sender.send(msg).await;
        match receiver.await {
            Err(_) => Err("Error adding member".into()),
            Ok(_) => {
                log::info!("New member added: {}", member);
                Ok(())
            }
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

    pub async fn get_members(&self) -> Result<ReliableSenderMap, Box<dyn Error>> {
        let (respond_to, receiver) = oneshot::channel();
        if self
            .sender
            .send(MembershipMessage::GetMembers(respond_to))
            .await
            .is_err()
        {
            return Err("Error sending request to get members".into());
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
        let membership_handle = MembershipHandle::start("localhost".to_string()).await;
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
        let membership_handle = MembershipHandle::start("localhost".to_string()).await;

        assert!(membership_handle
            .remove_member("localhost22".to_string())
            .await
            .is_err());
    }

    #[tokio::test]
    async fn it_should_return_members_as_empty_vec() {
        let membership_handle = MembershipHandle::start("localhost".to_string()).await;

        let reliable_senders = membership_handle.get_members().await;
        assert!(reliable_senders.unwrap().is_empty());
    }

    #[tokio::test]
    async fn it_should_return_members_as_vec() {
        let membership_handle = MembershipHandle::start("localhost".to_string()).await;
        let mut reliable_sender_handle = ReliableSenderHandle::default();
        reliable_sender_handle
            .expect_clone()
            .returning(ReliableSenderHandle::default);
        let _ = membership_handle
            .add_member("localhost".to_string(), reliable_sender_handle)
            .await;

        let reliable_senders = membership_handle.get_members().await;
        assert_eq!(reliable_senders.unwrap().len(), 1);
    }
}
