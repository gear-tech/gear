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

use super::{InitConfig, v0};
use crate::RawDatabase;
use alloy::providers::{Provider as _, RootProvider};
use anyhow::{Context as _, Result};
use ethexe_common::{
    ProtocolTimelines,
    db::{DBConfig, DBGlobals},
};
use gprimitives::H256;
use parity_scale_codec::{Decode, Encode};

pub const VERSION: u32 = 1;

const _: () = const {
    assert!(
        crate::VERSION == super::v2::VERSION,
        "Check migration code for types changing in case of version change: DBConfig, DBGlobals, ProtocolTimelines"
    );
};

pub async fn migration_from_v0(config: &InitConfig, db: &RawDatabase) -> Result<()> {
    // Changes from version 0 to version 1:
    // 1) LatestData is removed, and some fields are moved to DBGlobals
    //    DB keys have the same prefix, but appends 8 zero bytes in the end.
    // 2) Timelines is moved to more common DBConfig.
    //    DB keys have the same prefix, but appends 8 zero bytes in the end.

    let provider: RootProvider = RootProvider::connect(&config.ethereum_rpc).await?;
    let chain_id = provider.get_chain_id().await?;

    let latest_data_key = H256::from_low_u64_be(14);
    let timelines_key = H256::from_low_u64_be(15);

    let globals_key = [H256::from_low_u64_be(14).0.as_slice(), &[0u8; 8]].concat();
    let config_key = [H256::from_low_u64_be(15).0.as_slice(), &[0u8; 8]].concat();

    let latest_data = unsafe { db.kv.take(latest_data_key.as_bytes()) }
        .with_context(|| format!("latest data not found for db at version {}", v0::VERSION))
        .map(|bytes| v0::LatestData::decode(&mut bytes.as_slice()))?
        .context("failed to decode LatestData during migration")?;

    let globals = DBGlobals {
        start_block_hash: latest_data.start_block_hash,
        start_announce_hash: latest_data.start_announce_hash,
        latest_synced_block: latest_data.synced_block,
        latest_prepared_block_hash: latest_data.prepared_block_hash,
        latest_computed_announce_hash: latest_data.computed_announce_hash,
    };

    db.kv.put(&globals_key, globals.encode());

    let timelines = unsafe { db.kv.take(timelines_key.as_bytes()) }
        .context("timelines not found for db at version 0")
        .map(|bytes| v0::ProtocolTimelines::decode(&mut bytes.as_slice()))?
        .context("failed to decode ProtocolTimelines during migration")?;

    let db_config = DBConfig {
        version: VERSION,
        chain_id,
        router_address: config.router_address,
        timelines: ProtocolTimelines {
            genesis_ts: timelines.genesis_ts,
            era: timelines.era,
            election: timelines.election,
            slot: config.slot_duration_secs,
        },
        genesis_block_hash: latest_data.genesis_block_hash,
        genesis_announce_hash: latest_data.genesis_announce_hash,
    };

    db.kv.put(&config_key, db_config.encode());

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::migrations::test::assert_migration_types_hash;
    use scale_info::meta_type;

    #[test]
    fn ensure_migration_types() {
        assert_migration_types_hash(
            "v0->v1",
            vec![
                meta_type::<v0::LatestData>(),
                meta_type::<DBGlobals>(),
                meta_type::<v0::ProtocolTimelines>(),
                meta_type::<DBConfig>(),
            ],
            "68246d1aef14df71d8ba42d0a3b81f87c51c58c6ab24fe0348f1882e9c7d5a5a",
        );
    }
}
