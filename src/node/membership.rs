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
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct Membership {
    members: Arc<Mutex<HashMap<String, ReliableSenderHandle>>>,
}

impl Membership {
    pub fn new() -> Self {
        Self {
            members: Arc::new(Mutex::new(HashMap::default())),
        }
    }

    pub fn add_member(&self, addr: String, handle: ReliableSenderHandle) {
        self.members.lock().unwrap().insert(addr, handle);
    }

    pub fn remove_member(&self, addr: String) {
        self.members.lock().unwrap().remove(&addr);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn it_should_create_membership_add_and_remove_members() {
        let membership = Membership::new();
        let reliable_sender_handle = ReliableSenderHandle::default();
        let reliable_sender_handle_2 = ReliableSenderHandle::default();

        membership.add_member("localhost".to_string(), reliable_sender_handle);

        membership.add_member("localhost2".to_string(), reliable_sender_handle_2);

        membership.remove_member("localhost".to_string());
    }
}
