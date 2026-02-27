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

use super::{InitConfig, v0, v1};
use alloy::providers::{Provider as _, RootProvider};
use anyhow::{Context as _, Result, anyhow};
use ethexe_common::ProtocolTimelines;
use ethexe_db::DatabaseRef;
use gprimitives::H256;
use parity_scale_codec::{Decode, Encode};

pub const VERSION: u32 = 1;

pub async fn migration_from_v0(config: &InitConfig, db: &DatabaseRef<'_, '_>) -> Result<()> {
    log::info!(
        "Migrating the database from version {} to version {}",
        v0::VERSION,
        v1::VERSION
    );
    // Changes from version 0 to version 1:
    // 1) LatestData is removed, and some fields are moved to DBGlobals
    //    DB keys have the same prefix, but appends 8 zero bytes in the end.
    // 2) Timelines is moved to more common DBConfig.
    //    DB keys have the same prefix, but appends 8 zero bytes in the end.

    let provider: RootProvider = RootProvider::connect(&config.ethereum_rpc).await.unwrap();
    let chain_id = provider.get_chain_id().await?;

    let latest_data_key = H256::from_low_u64_be(14);
    let timelines_key = H256::from_low_u64_be(15);

    let globals_key = [H256::from_low_u64_be(14).0.as_slice(), &[0u8; 8]].concat();
    let config_key = [H256::from_low_u64_be(15).0.as_slice(), &[0u8; 8]].concat();

    let latest_data = db
        .kv
        .get(latest_data_key.as_bytes())
        .ok_or_else(|| anyhow!("latest data not found for db at version {}", v0::VERSION))
        .map(|bytes| v0::LatestData::decode(&mut bytes.as_slice()))?
        .context("failed to decode LatestData during migration")?;

    let globals = ethexe_common::db::DBGlobals {
        start_block_hash: latest_data.start_block_hash,
        start_announce_hash: latest_data.start_announce_hash,
        latest_synced_block: latest_data.synced_block,
        latest_prepared_block_hash: latest_data.prepared_block_hash,
        latest_computed_announce_hash: latest_data.computed_announce_hash,
    };

    db.kv.put(&globals_key, globals.encode());

    let timelines = db
        .kv
        .get(timelines_key.as_bytes())
        .ok_or_else(|| anyhow!("timelines not found for db at version 0"))
        .map(|bytes| v0::ProtocolTimelines::decode(&mut bytes.as_slice()))?
        .context("failed to decode ProtocolTimelines during migration")?;

    let db_config = ethexe_common::db::DBConfig {
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
    use crate::migration::test::assert_migration_types_hash;
    use scale_info::meta_type;

    #[test]
    fn ensure_migration_types() {
        assert_migration_types_hash(
            "v0->v1",
            vec![
                meta_type::<v0::LatestData>(),
                meta_type::<ethexe_common::db::DBGlobals>(),
                meta_type::<v0::ProtocolTimelines>(),
                meta_type::<ethexe_common::db::DBConfig>(),
            ],
            "3177a0fb8ad47482d0ab5d4898b2fa6702730fe3e77fffcd24ab0901d6a5413d",
        );
    }
}
