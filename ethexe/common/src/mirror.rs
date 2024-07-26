// This file is part of Gear.
//
// Copyright (C) 2024 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use alloc::vec::Vec;
use gprimitives::{ActorId, MessageId, H256};

/* Events section */

pub enum Event {
    ClaimValueRequested {
        claimed_id: MessageId,
        source: ActorId,
    },
    ExecutableBalanceTopUpRequested {
        value: u128,
    },
    Message {
        id: MessageId,
        destination: ActorId,
        payload: Vec<u8>,
        value: u128,
    },
    MessageQueueingRequested {
        id: MessageId,
        source: ActorId,
        payload: Vec<u8>,
        value: u128,
    },
    Reply {
        payload: Vec<u8>,
        value: u128,
        reply_to: MessageId,
        // TODO (breathx): use `gear_core::ReplyCode`.
        reply_code: [u8; 4],
    },
    ReplyQueueingRequested {
        replied_to: MessageId,
        source: ActorId,
        payload: Vec<u8>,
        value: u128,
    },
    StateChanged {
        state_hash: H256,
    },
    ValueClaimed {
        claimed_id: MessageId,
        value: u128,
    },
}
