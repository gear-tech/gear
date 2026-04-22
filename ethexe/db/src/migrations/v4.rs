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
use crate::RawDatabase;
use anyhow::{Context as _, Result};
use ethexe_common::db::DBConfig;
use gprimitives::H256;

pub const VERSION: u32 = 4;

const _: () = const {
    assert!(
        crate::VERSION == VERSION,
        "Check migration code for types changing in case of version change: DBConfig"
    );
};

/// v3 → v4: `InstrumentedCode` key layout gained a second `u32` slot
/// (runtime_id, code_id) → (runtime_id, version, code_id). Drop any entries
/// stored under the old 2-tuple layout so the cluster re-instruments them on
/// next code observation. `OriginalCode` is preserved in CAS, so reprocessing
/// does not need to re-fetch.
///
/// Note: this migration only cleans stale DB state. It does not itself
/// re-instrument existing codes — that relies on the normal code processing
/// pipeline observing the code again. Operators upgrading a live node should
/// expect a brief window where existing programs return
/// `MissingInstrumentedCodeForProgram` until their codes are re-observed.
pub async fn migration_from_v3(_: &InitConfig, db: &RawDatabase) -> Result<()> {
    // Matches `database::Key::InstrumentedCode`'s u64 discriminant (= 8).
    const INSTRUMENTED_CODE_DISCRIMINANT: u64 = 8;
    const PREFIX_LEN: usize = std::mem::size_of::<H256>();
    // Old body: runtime_id(u32) + code_id(32 bytes).
    const OLD_BODY_LEN: usize = std::mem::size_of::<u32>() + 32;

    let prefix = H256::from_low_u64_be(INSTRUMENTED_CODE_DISCRIMINANT);

    let old_keys: Vec<Vec<u8>> = db
        .kv
        .iter_prefix(prefix.as_bytes())
        .filter_map(|(k, _)| {
            // Old layout total length: prefix(32) + body(36) = 68 bytes.
            (k.len() == PREFIX_LEN + OLD_BODY_LEN).then_some(k)
        })
        .collect();

    let deleted = old_keys.len();
    for key in old_keys {
        // SAFETY: these entries are unreadable under the new 3-tuple key
        // layout and the return value is intentionally discarded.
        unsafe {
            db.kv.take(&key);
        }
    }

    log::info!(
        "migration v3→v4: dropped {deleted} stale InstrumentedCode entries under old key layout"
    );

    let config = db.kv.config().context("Cannot find db config")?;
    db.kv.set_config(DBConfig {
        version: VERSION,
        ..config
    });

    Ok(())
}
