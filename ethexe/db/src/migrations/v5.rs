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

/// v4 → v5: `ethexe_runtime_common::VERSION` was bumped (1 → 2) so the WASM
/// instrumentation pipeline now strips custom sections before persisting
/// `InstrumentedCode`. Pre-bump entries under the old `VERSION` are
/// unreachable through the new key but still occupy disk; drop them.
///
/// There is no re-instrumentation mechanism today: the compute pipeline only
/// produces `InstrumentedCode` for codes it freshly observes from
/// `RouterEvent::CodeUploaded`. After this migration, every program whose
/// instrumented code was wiped will surface `MissingInstrumentedCodeForProgram`
/// on dispatch until the same `OriginalCode` is uploaded to the chain again.
/// `OriginalCode` itself stays put in CAS, so the data isn't lost — only the
/// derived bytes are. Acceptable pre-mainnet; if that ever changes, add a
/// lazy re-instrument path in `instrumented_code_and_metadata`.
pub async fn migration_from_v4(_: &InitConfig, db: &RawDatabase) -> Result<()> {
    // Matches `database::Key::InstrumentedCode`'s u64 discriminant (= 8).
    const INSTRUMENTED_CODE_DISCRIMINANT: u64 = 8;

    let prefix = H256::from_low_u64_be(INSTRUMENTED_CODE_DISCRIMINANT);

    // The post-bump key shape `(version, code_id)` has the same byte length as
    // the pre-bump one, so we can't filter stale entries by length. Every
    // entry under this discriminant was written at the previous `VERSION` —
    // wipe them all unconditionally.
    let stale_keys: Vec<Vec<u8>> = db
        .kv
        .iter_prefix(prefix.as_bytes())
        .map(|(k, _)| k)
        .collect();

    let deleted = stale_keys.len();
    for key in stale_keys {
        // SAFETY: every entry under the `InstrumentedCode` discriminant is
        // stale relative to the new `VERSION`; the return value is discarded.
        unsafe {
            db.kv.take(&key);
        }
    }

    log::info!("migration v4→v5: dropped {deleted} stale InstrumentedCode entries");

    let config = db.kv.config().context("Cannot find db config")?;
    db.kv.set_config(DBConfig {
        version: VERSION,
        ..config
    });

    Ok(())
}
