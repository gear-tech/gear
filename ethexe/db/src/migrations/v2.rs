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
use crate::{RawDatabase, database::BlockSmallData, migrations::v1};
use anyhow::{Context as _, Result, anyhow, ensure};
use ethexe_common::{
    Announce,
    db::{AnnounceStorageRW, DBConfig},
};
use gprimitives::H256;
use parity_scale_codec::Decode;

pub const VERSION: u32 = 2;

pub async fn migration_from_v1(_: &InitConfig, db: &RawDatabase) -> Result<()> {
    // Changes from version 1 to version 2:
    // Copy announces data to KV

    log::info!("Migration investigation pass started: not modifying any data in database");

    let config = db.kv.config().context("Cannot find db config")?;

    ensure!(
        config.version == v1::VERSION,
        "Expected database version {}, but found {}",
        v1::VERSION,
        config.version
    );

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

        let Some(BlockSmallData { meta, .. }) = BlockSmallData::decode(&mut v.as_slice()).ok()
        else {
            continue;
        };

        log::trace!("Investigating block {block_hash:?} with meta {meta:?}");

        let Some(announces) = meta.announces else {
            continue;
        };

        for announce_hash in announces {
            let data = db
                .cas
                .read(announce_hash.inner())
                .ok_or_else(|| anyhow!("found a block with missed announce in set"))?;
            let announce = Announce::decode(&mut data.as_slice())
                .context("failed to decode announce during migration")?;

            ensure!(
                announce.block_hash == block_hash,
                "announce block hash doesn't match block hash in meta during migration"
            );

            announces_to_copy.push(announce);
        }
    }

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
