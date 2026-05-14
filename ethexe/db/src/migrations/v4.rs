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

use super::{InitConfig, utils};
use crate::RawDatabase;
use alloy::providers::RootProvider;
use anyhow::{Context as _, Result, bail};
use ethexe_common::db::DBConfig;
use ethexe_ethereum::router::RouterQuery;
use parity_scale_codec::Decode;
use tracing::info;

pub const VERSION: u32 = 4;

const _: () = const {
    assert!(
        crate::VERSION == VERSION,
        "Check migration code for types changing in case of version change: DBConfig, DBGlobals, Announce, BlockSmallData. \
         Also check AnnounceStorageRW, KVDatabase, dyn KVDatabase implementations"
    );
};

pub async fn migration_from_v3(config: &InitConfig, db: &RawDatabase) -> Result<()> {
    info!("🚧 Starting database migration v3->v4");

    let provider = RootProvider::connect(&config.ethereum_rpc).await?;
    let router_query = RouterQuery::from_provider(config.router_address, provider);
    let storage_view = router_query.storage_view().await?;

    if storage_view.maxValidators == 0 {
        bail!("The maximum number of validators is set to 0 in Router. Check Router storage")
    }

    let key = utils::config_key_bytes();
    let raw_config = db.kv.get(&key).context("Database config not found")?;
    let old_config = migrated_types::DBConfig::decode(&mut raw_config.as_slice())
        .context("Failed to decode DBConfig")?;

    db.kv.set_config(DBConfig {
        version: VERSION,
        chain_id: old_config.chain_id,
        router_address: old_config.router_address,
        timelines: old_config.timelines,
        genesis_block_hash: old_config.genesis_block_hash,
        genesis_announce_hash: old_config.genesis_announce_hash,
        max_validators: storage_view.maxValidators,
    });

    info!("✅ Database migration v3->v4 successfully finished");
    Ok(())
}

/// Database types changes in `v4` migration.
pub mod migrated_types {
    use ethexe_common::{Address, Announce, HashOf, ProtocolTimelines};
    use gprimitives::H256;
    use parity_scale_codec::{Decode, Encode};
    use scale_info::TypeInfo;

    #[derive(Debug, Clone, Decode, Encode, TypeInfo)]
    pub struct DBConfig {
        pub version: u32,
        pub chain_id: u64,
        pub router_address: Address,
        pub timelines: ProtocolTimelines,
        pub genesis_block_hash: H256,
        pub genesis_announce_hash: HashOf<Announce>,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::migrations::migration::test::assert_migration_types_hash;
    use scale_info::meta_type;

    #[test]
    fn ensure_migration_types() {
        assert_migration_types_hash(
            "v3->v4",
            vec![meta_type::<migrated_types::DBConfig>()],
            "943384f31bb358ff3ce7691cf97710bc03ec7d75d20f03b8cc5cbffa7c4c00b0",
        );
    }
}
