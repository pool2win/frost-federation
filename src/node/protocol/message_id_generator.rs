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

use std::hash::{DefaultHasher, Hash, Hasher};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

#[derive(PartialEq, Debug, Eq, Hash, Clone, Serialize, Deserialize)]
pub struct MessageId(pub u64);

/// Calculate a simple u64 hash from string
///
/// Use the DefaultHasher provided with std for now.
fn calculate_hash<T: Hash>(t: &T) -> MessageId {
    let mut s = DefaultHasher::new();
    t.hash(&mut s);
    MessageId(s.finish())
}

/// A Message ID generator based on node id and local time
///
/// Use a simple Hash and DefaultHasher provided by std
#[derive(Clone, Debug)]
pub struct MessageIdGenerator {
    node_id: String,
}

impl MessageIdGenerator {
    /// Build a new id generator for the given node id
    pub fn new(node_id: String) -> Self {
        Self { node_id }
    }

    /// Generate an ID as a string
    ///
    /// Get local time in nano seconds
    /// Concatenate the node_id to the time
    pub fn next(&self) -> MessageId {
        let mut current_time = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
            .to_string();
        current_time.push_str(&self.node_id);
        calculate_hash(&current_time)
    }
}

#[cfg(test)]
mod message_id_generator_tests {
    use super::{MessageId, MessageIdGenerator};

    #[test]
    fn it_generates_ids() {
        let gen = MessageIdGenerator::new("some node id".to_string());
        assert_ne!(gen.next(), MessageId(0));
        assert_ne!(gen.next(), gen.next());
    }
}
