// This file is part of Gear.
//
// Copyright (C) 2026 Gear Technologies Inc.
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

use ethexe_common::{Announce, HashOf, SimpleBlockData};
use gprimitives::H256;
use parity_scale_codec::{Decode, Encode};
use scale_info::TypeInfo;

pub const VERSION: u32 = 0;

#[derive(Encode, Decode, TypeInfo)]
pub struct LatestData {
    pub synced_block: SimpleBlockData,
    pub prepared_block_hash: H256,
    pub computed_announce_hash: HashOf<Announce>,
    pub genesis_block_hash: H256,
    pub genesis_announce_hash: HashOf<Announce>,
    pub start_block_hash: H256,
    pub start_announce_hash: HashOf<Announce>,
}

#[derive(Encode, Decode, TypeInfo)]
pub struct ProtocolTimelines {
    pub genesis_ts: u64,
    pub era: u64,
    pub election: u64,
}
