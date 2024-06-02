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

use gear_core::ids::ProgramId;
use gprimitives::H256;
use parity_scale_codec::{Decode, Encode};

#[derive(Clone, Debug, Encode, Decode)]
pub struct Message {
    pub sender: ProgramId,
    pub gas_limit: u64,
    pub value: u128,
    pub data: Vec<u8>,
}

#[derive(Clone, Debug, Encode, Decode)]
pub struct Page {
    pub index: u32,
    pub data: Vec<u8>,
}

/// Hypercore program state.
#[derive(Clone, Debug, Decode, Encode)]
pub struct State {
    /// Program ID.
    pub program_id: ProgramId,
    pub queue: Vec<Message>,
    pub pages: Vec<Page>,
    pub original_code_hash: H256,
    pub instrumented_code_hash: H256,
}
