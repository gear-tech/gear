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

use crate::{
    InitConfig, RawDatabase,
    database::BlockSmallData,
    migrations::{v2, v3::keys::LATEST_ERA_VALIDATORS_COMMITTED_KEY_PREF},
};
use anyhow::{Context, Result, bail};
use ethexe_common::db::{BlockMeta, DBConfig};
use gprimitives::H256;
use parity_scale_codec::{Decode, Encode};
use tracing::{debug, info, warn};

pub const VERSION: u32 = 3;

/// Historical key prefixes for `v3` migration.
mod keys {
    pub const BLOCK_SMALL_DATA_KEY_PREF: u64 = 0;
    pub const LATEST_ERA_VALIDATORS_COMMITTED_KEY_PREF: u64 = 16;
    pub const BLOCK_ANNOUNCES_KEY_PREF: u64 = 18;
}

/// Changes from **v2** to **v3**:
/// 1. Block announces are moved from [BlockMeta] to [`ethexe_common::db::AnnounceStorageRO`], and
///    stores now by key `BlockAnnounces`
/// 2. `LatestEraValidators` key is merged into `BlockMeta`.
pub async fn migration_from_v2(_: &InitConfig, db: &RawDatabase) -> Result<()> {
    info!("🚧 Database migration v2->v3 starting...");
    let config = db.kv.config().context("Database config not found")?;

    if config.version != v2::VERSION {
        bail!(
            "Inconsistent database version: expected_version={}, found_version={}",
            v2::VERSION,
            config.version
        )
    }

    let block_small_data_prefix = H256::from_low_u64_be(keys::BLOCK_SMALL_DATA_KEY_PREF);
    let block_announces_prefix = H256::from_low_u64_be(keys::BLOCK_ANNOUNCES_KEY_PREF);
    let latest_era_prefix = H256::from_low_u64_be(LATEST_ERA_VALIDATORS_COMMITTED_KEY_PREF);

    let mut block_announces_copy = Vec::new();
    let mut block_small_data_copy = Vec::new();

    for (key, value) in db.kv.iter_prefix(block_small_data_prefix.as_bytes()) {
        if key.len() != 2 * std::mem::size_of::<H256>() {
            warn!(
                "⚠️ Found invalid BlockSmallData key: expected key len - {}, found key len - {}",
                2 * std::mem::size_of::<H256>(),
                key.len()
            );
            continue;
        }

        let block_small_data = v3_migrated_types::BlockSmallData::decode(&mut value.as_slice())
            .context("Failed to decode `v3_migrated_types::BlockSmallData` from database")?;

        let v3_migrated_types::BlockSmallData {
            block_header,
            block_is_synced,
            meta:
                v3_migrated_types::BlockMeta {
                    prepared,
                    announces,
                    codes_queue,
                    last_committed_batch,
                    last_committed_announce,
                },
        } = block_small_data;

        let block_hash = H256::from_slice(&key[std::mem::size_of::<H256>()..]);

        let latest_era_key = [latest_era_prefix.as_bytes(), block_hash.as_bytes()].concat();
        let Some(latest_era_raw) = db.kv.get(&latest_era_key) else {
            // Put the debug data in log, not in error message
            debug!(block_hash=%block_hash, block_small_data_key=?key, latest_era_key=?latest_era_key, "Latest era validators not found for block");

            bail!("`Latest era validators committed` not found for block={block_hash}")
        };

        let latest_era_validators_committed = u64::decode(&mut latest_era_raw.as_slice())
            .context("Failed to decode era number (u64)")?;

        let new_block_small_data = BlockSmallData {
            block_header,
            block_is_synced,
            meta: BlockMeta {
                prepared,
                codes_queue,
                last_committed_batch,
                last_committed_announce,
                latest_era_validators_committed,
            },
        };

        // Put new BlockSmallData by the same key.
        block_small_data_copy.push((key, new_block_small_data));
        // Put announces only if it contains some.
        if let Some(announces) = announces {
            block_announces_copy.push((block_hash, announces));
        }
    }

    info!("⏳ All migratable data successfully collected");

    for (block_hash, announces) in block_announces_copy {
        let block_announces_key =
            [block_announces_prefix.as_bytes(), block_hash.as_bytes()].concat();
        db.kv.put(&block_announces_key, announces.encode());
    }

    for (key, block_small_data) in block_small_data_copy {
        db.kv.put(&key, block_small_data.encode());
    }

    info!("⏳ All migrated data updated in database");

    db.kv.set_config(DBConfig {
        version: VERSION,
        ..config
    });

    info!("✅ Database config updated. Migration v2->v3 successfully finished.");

    Ok(())
}

pub mod v3_migrated_types {

    use ethexe_common::{Announce, BlockHeader, HashOf};
    use gsigner::Digest;
    // TODO: check the data structures.
    // Before migration is was: use `alloc::collections::BTreeSet`
    use gear_core::ids::CodeId;
    use parity_scale_codec::{Decode, Encode};
    use scale_info::TypeInfo;
    use std::collections::{BTreeSet, VecDeque};

    /// [BlockMeta] type used before v3 migration.
    #[derive(Clone, Debug, Default, Encode, Decode, TypeInfo, PartialEq, Eq, Hash)]
    pub struct BlockMeta {
        pub prepared: bool,
        pub announces: Option<BTreeSet<HashOf<Announce>>>,
        pub codes_queue: Option<VecDeque<CodeId>>,
        pub last_committed_batch: Option<Digest>,
        pub last_committed_announce: Option<HashOf<Announce>>,
    }

    /// [BlockSmallData] type used before v3 migration.
    #[derive(Debug, Clone, Default, Encode, Decode, PartialEq, Eq, TypeInfo)]
    pub struct BlockSmallData {
        pub block_header: Option<BlockHeader>,
        pub block_is_synced: bool,
        pub meta: BlockMeta,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::migrations::test::assert_migration_types_hash;
    use ethexe_common::db::DBConfig;
    use scale_info::meta_type;

    #[test]
    fn ensure_migration_types() {
        assert_migration_types_hash(
            "v2->v3",
            vec![
                meta_type::<DBConfig>(),
                meta_type::<v3_migrated_types::BlockSmallData>(),
                meta_type::<v3_migrated_types::BlockMeta>(),
                meta_type::<BlockSmallData>(),
                meta_type::<BlockMeta>(),
            ],
            "a473452904e3947bda043fa55c88c74c5d3d603fa4ca8ca72a807542d1cd02eb",
        );
    }
}
