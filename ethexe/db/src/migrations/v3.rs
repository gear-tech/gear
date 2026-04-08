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

use crate::{InitConfig, RawDatabase};
use anyhow::{Context as _, Result, bail};
use ethexe_common::{
    HashOf,
    db::{AnnounceStorageRW, InjectedStorageRO, InjectedStorageRW},
    injected::AnnounceInjectedTransaction,
};
use gprimitives::H256;
use log::{info, warn};
use parity_scale_codec::{Decode, Encode};

pub const VERSION: u32 = 3;

const _: () = const {
    assert!(crate::VERSION == VERSION);
};

pub async fn migration_from_v2(_: &InitConfig, db: &RawDatabase) -> Result<()> {
    // TODO: `Announce` keeps the same hash, but its stored value format changed:
    // `Vec<SignedInjectedTransaction>` was replaced with
    // `Vec<AnnounceInjectedTransaction>`.
    //
    // The migration must:
    // 1. Read announces using the previous schema.
    // 2. Backfill injected transaction storage with full signed transactions.
    // 3. Rewrite announces into the new compact schema.
    // 4. Verify `Announce::to_hash()` remains unchanged for migrated values.

    info!("Migrating from v2 to v3");

    // Migratable data
    let mut announces_to_insert = Vec::new();
    let mut transactions_to_insert = Vec::new();

    const H256_SIZE: usize = std::mem::size_of::<H256>();
    const ANNOUNCE_KEY_PREFIX: u64 = 17;
    let announce_prefix = H256::from_low_u64_be(ANNOUNCE_KEY_PREFIX);
    db.kv.iter_prefix(announce_prefix.as_bytes()).try_for_each(|(key, value)| {
        if key.len() != H256_SIZE * 2 {
            warn!("Migration from v2 to v3: invalid announce key len, expected len={}, got len={}", H256_SIZE * 2, key.len());
            return Ok(());
        }
        let announce_hash = H256::from_slice(&key[H256_SIZE..]) ;
        let announce_v2 = v2_types::Announce::decode(&mut value.as_slice()).context("failed to decode v2_types::Announce during migration")?;

        let v2_types::Announce {block_hash, parent, gas_allowance, injected_transactions} = announce_v2;
        let migrated_announce = ethexe_common::Announce {
            block_hash,
            // The hash of announce do not changed.
            parent: unsafe {HashOf::new(parent.clone().inner())},
            gas_allowance,
            injected_transactions: injected_transactions.iter().map(|tx| AnnounceInjectedTransaction::from_signed_tx(tx)).collect()
        };
        let migrated_announce_hash = migrated_announce.to_hash().inner();

        if migrated_announce_hash !=announce_hash {
            bail!("Migrated announce hash does not match the original hash: original_hash={announce_hash}, migrated_hash={migrated_announce_hash}");
        }

        injected_transactions.into_iter().for_each(|tx| {
           let tx_hash = tx.data().to_hash();
           // If transaction is not found in database, it should be inserted.
           if db.injected_transaction(tx_hash).is_none() {
               transactions_to_insert.push(tx);
           }
        });

        announces_to_insert.push(migrated_announce);
        return Ok(());
    })?;

    info!("✅ Migratable data from v2 to v3 prepared successfully");
    info!("⏳ Inserting migrated transactions and announces into the database...");

    announces_to_insert.into_iter().for_each(|announce| {
        db.set_announce(announce);
    });

    transactions_to_insert.into_iter().for_each(|tx| {
        db.set_injected_transaction(tx);
    });

    Ok(())
}

/// Module provides the original types used in the v2 database schema.
mod v2_types {
    use super::*;
    use ethexe_common::{HashOf, injected::SignedInjectedTransaction};

    #[derive(Clone, Debug, Encode, Decode, PartialEq, Eq)]
    pub struct Announce {
        pub block_hash: H256,
        pub parent: HashOf<Self>,
        pub gas_allowance: Option<u64>,
        pub injected_transactions: Vec<SignedInjectedTransaction>,
    }
}
