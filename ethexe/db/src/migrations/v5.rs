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

pub const VERSION: u32 = 5;

const _: () = const {
    assert!(
        crate::VERSION == VERSION,
        "Check migration code for types changing in case of version change: DBConfig"
    );
};

/// v4 → v5: drop every `InstrumentedCode` entry. `ethexe_runtime_common::VERSION`
/// was bumped, so all prior entries are unreachable through the new key.
///
/// There is no re-instrumentation mechanism: the compute pipeline only
/// produces `InstrumentedCode` for codes it observes from
/// `RouterEvent::CodeUploaded`. Affected programs surface
/// `MissingInstrumentedCodeForProgram` on dispatch until their `OriginalCode`
/// is uploaded again. `OriginalCode` itself stays in CAS.
pub async fn migration_from_v4(_: &InitConfig, db: &RawDatabase) -> Result<()> {
    // Matches `database::Key::InstrumentedCode`'s u64 discriminant (= 8). The
    // accompanying test in `database.rs` pins this value.
    const INSTRUMENTED_CODE_DISCRIMINANT: u64 = 8;

    let prefix = H256::from_low_u64_be(INSTRUMENTED_CODE_DISCRIMINANT);

    let stale_keys: Vec<Vec<u8>> = db
        .kv
        .iter_prefix(prefix.as_bytes())
        .map(|(k, _)| k)
        .collect();

    let deleted = stale_keys.len();
    for key in stale_keys {
        // `KVDatabase::take` is unsafe purely for the data-loss risk — that's
        // exactly the intent here.
        let _ = unsafe { db.kv.take(&key) };
    }

    log::info!("migration v4→v5: dropped {deleted} stale InstrumentedCode entries");

    let config = db.kv.config().context("Cannot find db config")?;
    db.kv.set_config(DBConfig {
        version: VERSION,
        ..config
    });

    Ok(())
}
