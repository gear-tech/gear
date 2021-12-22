// This file is part of Gear.

// Copyright (C) 2021 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use blake2_rfc::blake2b;
use gear_core::{
    message::{MessageId, MessageIdGenerator},
    program::ProgramId,
};

pub struct BlakeMessageIdGenerator {
    pub program_id: ProgramId,
    pub nonce: u64,
}

impl MessageIdGenerator for BlakeMessageIdGenerator {
    fn next(&mut self) -> MessageId {
        let mut data = self.program_id.as_slice().to_vec();
        data.extend(&self.nonce.to_le_bytes());

        self.nonce += 1;

        MessageId::from_slice(blake2b::blake2b(32, &[], &data).as_bytes())
    }

    fn current(&self) -> u64 {
        self.nonce
    }
}

pub fn next_message_id(program_id: ProgramId, nonce: u64) -> MessageId {
    BlakeMessageIdGenerator { program_id, nonce }.next()
}
