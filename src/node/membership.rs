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
use actix::prelude as actix;
use std::collections::HashMap;

pub type ReliableSenderMap = HashMap<String, ReliableSenderHandle>;

pub(crate) struct AddMember {
    pub(crate) member: String,
    pub(crate) handler: ReliableSenderHandle,
}

pub(crate) struct RemoveMember {
    pub(crate) member: String,
}

pub(crate) struct GetMembers;

impl actix::Message for AddMember {
    type Result = Result<(), std::io::Error>;
}

impl actix::Message for RemoveMember {
    type Result = Result<(), std::io::Error>;
}

impl actix::Message for GetMembers {
    type Result = Result<ReliableSenderMap, std::io::Error>;
}

pub(crate) struct Membership {
    members: ReliableSenderMap,
}

impl Default for Membership {
    fn default() -> Self {
        Membership {
            members: HashMap::new(),
        }
    }
}

impl actix::Actor for Membership {
    type Context = actix::Context<Self>;

    fn started(&mut self, _ctx: &mut actix::Context<Self>) {
        self.members = HashMap::new();
        log::debug!("Membership Actor is alive");
    }
}

impl actix::Handler<AddMember> for Membership {
    type Result = Result<(), std::io::Error>;

    fn handle(
        &mut self,
        msg: AddMember,
        _ctx: &mut actix::Context<Self>,
    ) -> Result<(), std::io::Error> {
        self.members.insert(msg.member, msg.handler);
        Ok(())
    }
}

impl actix::Handler<RemoveMember> for Membership {
    type Result = Result<(), std::io::Error>;

    fn handle(
        &mut self,
        msg: RemoveMember,
        _ctx: &mut actix::Context<Self>,
    ) -> Result<(), std::io::Error> {
        if self.members.remove(&msg.member).is_none() {
            Err(std::io::Error::other("No such member"))
        } else {
            Ok(())
        }
    }
}

impl actix::Handler<GetMembers> for Membership {
    type Result = Result<ReliableSenderMap, std::io::Error>;

    fn handle(
        &mut self,
        _msg: GetMembers,
        _ctx: &mut actix::Context<Self>,
    ) -> Result<ReliableSenderMap, std::io::Error> {
        Ok(self.members.clone())
    }
}

#[cfg(test)]
mod membership_tests {
    use ::actix::prelude as actix;
    use actix::Actor;

    use super::*;
    #[mockall_double::double]
    use crate::node::reliable_sender::ReliableSenderHandle;

    #[::actix::test]
    async fn it_should_create_membership_add_and_remove_members() {
        let membership_addr = Membership::default().start();
        let reliable_sender_handle = ReliableSenderHandle::default();
        let reliable_sender_handle_2 = ReliableSenderHandle::default();

        let _ = membership_addr
            .send(AddMember {
                member: "localhost".to_string(),
                handler: reliable_sender_handle,
            })
            .await;

        let _ = membership_addr
            .send(AddMember {
                member: "localhost2".to_string(),
                handler: reliable_sender_handle_2,
            })
            .await;

        assert!(membership_addr
            .send(RemoveMember {
                member: "localhost".to_string()
            })
            .await
            .is_ok());
    }

    #[::actix::test]
    async fn it_should_result_in_error_when_removing_non_member() {
        let _ = env_logger::builder().is_test(true).try_init();
        let membership_addr = Membership::default().start();

        let res = membership_addr
            .send(RemoveMember {
                member: "localhost22".to_string(),
            })
            .await;
        assert!(res.unwrap().is_err());
    }

    #[::actix::test]
    async fn it_should_return_members_as_empty_vec() {
        let membership_addr = Membership::default().start();

        let reliable_senders = membership_addr.send(GetMembers {}).await;
        assert!(reliable_senders.unwrap().unwrap().is_empty());
    }

    #[::actix::test]
    async fn it_should_return_members_as_vec() {
        let membership_addr = Membership::default().start();
        let mut reliable_sender_handle = ReliableSenderHandle::default();
        reliable_sender_handle
            .expect_clone()
            .returning(ReliableSenderHandle::default);
        let _ = membership_addr
            .send(AddMember {
                member: "localhost".to_string(),
                handler: reliable_sender_handle,
            })
            .await;

        let reliable_senders = membership_addr.send(GetMembers).await;
        assert_eq!(reliable_senders.unwrap().unwrap().len(), 1);
    }
}
