// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

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
