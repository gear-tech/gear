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

use crate::{InitConfig, RawDatabase, database::BlockSmallData, migrations::v0};
use anyhow::{Context, Result};
use ethexe_common::db::{BlockMeta, DBConfig};
use gprimitives::H256;
use parity_scale_codec::{Decode, Encode};

pub const VERSION: u32 = 2;

pub async fn migration_from_v1(_: &InitConfig, db: &RawDatabase) -> Result<()> {
    // Changes from v1 to v2:
    // - Block announces are moved from `BlockMeta` to `BlockAnnounces` key.
    // - `LatestEraValidators` key is merged into `BlockMeta`.

    let block_small_data_prefix = H256::from_low_u64_be(0);
    let block_announces_prefix = H256::from_low_u64_be(13);
    let latest_era_prefix = H256::from_low_u64_be(16);

    for (key, value) in db.kv.iter_prefix(block_small_data_prefix.as_bytes()) {
        let v0::BlockSmallData {
            block_header,
            block_is_synced,
            meta:
                v0::BlockMeta {
                    prepared,
                    announces,
                    codes_queue,
                    last_committed_batch,
                    last_committed_announce,
                },
        } = v0::BlockSmallData::decode(&mut value.as_slice())?;

        let block_hash = &key[32..];

        let announces_key = [block_announces_prefix.as_bytes(), block_hash].concat();
        let latest_era_key = [latest_era_prefix.as_bytes(), block_hash].concat();

        let latest_era_validators_committed = db
            .kv
            .get(&latest_era_key)
            .context("`LatestEraValidators` is not found for block")
            .and_then(|bytes| Ok(u64::decode(&mut bytes.as_slice())?))?;

        db.kv.put(&announces_key, announces.encode());

        db.kv.put(
            &key,
            BlockSmallData {
                block_header,
                block_is_synced,
                meta: BlockMeta {
                    prepared,
                    codes_queue,
                    last_committed_batch,
                    last_committed_announce,
                    latest_era_validators_committed,
                },
            }
            .encode(),
        );
    }

    let config_key = [H256::from_low_u64_be(15).0.as_slice(), &[0u8; 8]].concat();

    let old_config = db
        .kv
        .get(&config_key)
        .context("Database config are guaranteed for version 1, but not found")
        .and_then(|bytes| Ok(DBConfig::decode(&mut bytes.as_slice())?))?;

    db.kv.put(
        &config_key,
        DBConfig {
            version: VERSION,
            ..old_config
        }
        .encode(),
    );

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
            "v1->v2",
            vec![
                meta_type::<DBConfig>(),
                meta_type::<v0::BlockSmallData>(),
                meta_type::<v0::BlockMeta>(),
                meta_type::<BlockSmallData>(),
                meta_type::<BlockMeta>(),
            ],
            "6506461993fe4e74645148eb4af27aecfef09e5b4789b5b9936c86adab62a8ff",
        );
    }
}
