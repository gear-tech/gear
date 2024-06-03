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

//! State-related data structures.

use std::collections::{BTreeMap, BTreeSet};

use gear_core::{ids::ProgramId, pages::GearPage, code::InstrumentedCode};
use gprimitives::H256;
use parity_scale_codec::{Decode, Encode};

/// Hypercore program state.
#[derive(Clone, Debug, Decode, Encode)]
pub struct ProgramState {
    /// Hash of incoming message queue, see [`MessageQueue`].
    pub queue_hash: H256,
    /// Hash of memory pages table, see [`MemoryPages`].
    pub pages_hash: H256,
    /// Hash of the original code bytes.
    pub original_code_hash: H256,
    /// Hash of the instrumented code, see [`InstrumentedCode`].
    pub instrumented_code_hash: H256,
}

#[derive(Clone, Debug, Encode, Decode)]
pub struct Message {
    pub sender: ProgramId,
    pub gas_limit: u64,
    pub value: u128,
    /// Hash of payload bytes.
    pub payload_hash: H256,
}

#[derive(Clone, Debug, Encode, Decode)]
pub struct MessageQueue(pub Vec<Message>);

/// Memory pages table, mapping gear page number to page data bytes hash.
#[derive(Clone, Debug, Encode, Decode)]
pub struct MemoryPages(pub BTreeMap<GearPage, H256>);


