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

/// Handlers to query/update node state
#[derive(Clone)]
pub(crate) struct State {
    pub(crate) membership_handle: MembershipHandle,
}

impl State {
    pub fn new(membership_handle: MembershipHandle) -> Self {
        Self { membership_handle }
    }
}
