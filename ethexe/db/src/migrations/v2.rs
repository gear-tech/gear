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

use super::InitConfig;
use anyhow::{Context as _, Result, ensure};
use gprimitives::H256;
use parity_scale_codec::Decode;

// Critical usages for migration
#[allow(unused_imports)]
use crate::KVDatabase;
use crate::{
    RawDatabase,
    migrations::{v3, v3::v3_migrated_types},
};
use ethexe_common::{
    Announce, HashOf,
    db::{AnnounceStorageRW, DBConfig, DBGlobals},
};

pub const VERSION: u32 = 2;

const _: () = const {
    assert!(
        crate::VERSION == v3::VERSION,
        "Check migration code for types changing in case of version change: DBConfig, DBGlobals, Announce, BlockSmallData. \
         Also check AnnounceStorageRW, KVDatabase, dyn KVDatabase implementations"
    );
};

pub async fn migration_from_v1(_: &InitConfig, db: &RawDatabase) -> Result<()> {
    // Changes from version 1 to version 2: copying announces data to KV

    log::info!("Migration investigation pass started: not modifying any data in database");

    let cas_copy = db.cas.clone_boxed();
    let get_announce_from_cas = move |announce_hash: HashOf<Announce>| {
        cas_copy
            .read(announce_hash.inner())
            .and_then(|data| Announce::decode(&mut data.as_slice()).ok())
            .context("cannot get announce from CAS")
    };

    const BLOCK_SMALL_DATA_PREFIX: u64 = 0x00;
    let mut announces_to_copy = Vec::new();
    for (k, v) in db
        .kv
        .iter_prefix(H256::from_low_u64_be(BLOCK_SMALL_DATA_PREFIX).as_bytes())
    {
        if k.len() != 2 * std::mem::size_of::<H256>() {
            continue;
        }

        let block_hash = H256::from_slice(&k[std::mem::size_of::<H256>()..]);

        let v3_migrated_types::BlockSmallData { meta, .. } =
            v3_migrated_types::BlockSmallData::decode(&mut v.as_slice())
                .context("failed to decode BlockSmallData during migration")?;

        log::trace!("Investigating block {block_hash:?} with meta {meta:?}");

        for announce_hash in meta.announces.into_iter().flatten() {
            let announce = get_announce_from_cas(announce_hash)
                .with_context(|| format!("cannot get announce by {announce_hash:?}"))?;

            ensure!(
                announce.block_hash == block_hash,
                "announce block hash doesn't match block hash in meta during migration"
            );

            ensure!(
                announce.to_hash() == announce_hash,
                "announce hash changes is unsupported in this migration"
            );

            announces_to_copy.push(announce);
        }
    }

    let config = db.kv.config().context("Cannot find db config")?;
    let globals: DBGlobals = db.kv.globals().context("Cannot find db globals")?;

    // Check that announce hashes in config and globals are correct, to be sure that we won't break anything by copying announces
    let genesis_announce_hash = get_announce_from_cas(config.genesis_announce_hash)
        .context("Cannot find genesis announce in CAS")?;
    let start_announce_hash = get_announce_from_cas(globals.start_announce_hash)
        .context("Cannot find start announce in CAS")?;
    let latest_computed_announce_hash =
        get_announce_from_cas(globals.latest_computed_announce_hash)
            .context("Cannot find latest computed announce in CAS")?;
    ensure!(
        genesis_announce_hash.to_hash() == config.genesis_announce_hash,
        "Unsupported: genesis announce hash changed"
    );
    ensure!(
        start_announce_hash.to_hash() == globals.start_announce_hash,
        "Unsupported: start announce hash changed"
    );
    ensure!(
        latest_computed_announce_hash.to_hash() == globals.latest_computed_announce_hash,
        "Unsupported: latest computed announce hash changed"
    );

    log::info!(
        "Migration investigation pass finished: found {} announces to copy, starting copy process",
        announces_to_copy.len()
    );

    for announce in announces_to_copy {
        db.set_announce(announce);
    }

    db.kv.set_config(DBConfig {
        version: VERSION,
        ..config
    });

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::migrations::{test::assert_migration_types_hash, v3::v3_migrated_types};
    use scale_info::meta_type;

    #[test]
    fn ensure_migration_types() {
        assert_migration_types_hash(
            "v1->v2",
            vec![
                meta_type::<DBConfig>(),
                meta_type::<DBGlobals>(),
                meta_type::<Announce>(),
                meta_type::<v3_migrated_types::BlockSmallData>(),
            ],
            "6cfad404549d4a146ccb2fbad83d63474383b2059363c45e740c842ac95f7c45",
        );
    }
}
