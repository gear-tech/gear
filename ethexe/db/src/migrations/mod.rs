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

#[cfg(feature = "mock")]
use crate::{Database, MemDb};
use crate::{RawDatabase, dump::StateDump};
use anyhow::{Context, Result};
use futures::future::BoxFuture;
use gear_core::code::{CodeMetadata, InstrumentedCode};
use gprimitives::CodeId;
use gsigner::Address;

pub use init::initialize_db;

mod init;

/// Latest on-disk schema version. Databases on older versions are upgraded
/// by running each [`Migration`] in [`migrations()`] in ascending order.
pub const LATEST_VERSION: u32 = 5;

pub type CodeProcessingFuture =
    BoxFuture<'static, anyhow::Result<Option<(InstrumentedCode, CodeMetadata)>>>;

pub trait GenesisInitializer {
    fn get_genesis_data(&mut self) -> anyhow::Result<StateDump>;
    fn process_code(&mut self, code_id: CodeId, code: Vec<u8>) -> CodeProcessingFuture;
}

pub struct InitConfig {
    pub ethereum_rpc: String,
    pub router_address: Address,
    pub slot_duration_secs: u64,
    pub genesis_initializer: Option<Box<dyn GenesisInitializer>>,
}

#[cfg(feature = "mock")]
pub async fn create_initialized_empty_memory_db(config: InitConfig) -> anyhow::Result<Database> {
    let raw = RawDatabase::from_one(&MemDb::default());
    init::initialize_empty_db(config, &raw).await?;
    Database::try_from_raw(raw)
}

/// A single on-disk schema upgrade step.
///
/// Each migration bumps the database from `source_version()` to
/// `source_version() + 1`. Implementations must be idempotent on the
/// migration target version: running the same migration twice in a row
/// against a `source_version() + 1` database must not corrupt state.
pub trait Migration {
    /// Schema version this migration upgrades from. The migration
    /// produces a database at version `source_version() + 1`.
    fn source_version(&self) -> u32;

    /// Apply the migration in-place to the raw database.
    fn migrate(&self, raw: &RawDatabase) -> Result<()>;
}

/// Returns the ordered list of in-tree migrations.
///
/// Populated as schema versions land. Earlier `v1`/`v2`/... entries were
/// dropped — they no longer apply to any live database, and they were
/// the only callers of removed APIs. New migrations should be appended
/// here in ascending order of `source_version`.
fn migrations() -> Vec<Box<dyn Migration>> {
    Vec::new()
}

/// Run every applicable migration to bring `raw` up to [`LATEST_VERSION`].
///
/// Returns the final version. Errors if any migration fails, or if the
/// resulting version doesn't reach [`LATEST_VERSION`].
pub fn migrate(raw: &RawDatabase) -> Result<u32> {
    let mut version = raw
        .kv
        .version()
        .context("failed to read database version")?
        .context("database has no version key")?;

    if version > LATEST_VERSION {
        anyhow::bail!(
            "database version {version} is newer than supported {LATEST_VERSION}; \
             refusing to downgrade"
        );
    }

    for m in migrations() {
        if version != m.source_version() {
            continue;
        }
        m.migrate(raw)
            .with_context(|| format!("migration from v{version} failed"))?;
        version += 1;
    }

    if version != LATEST_VERSION {
        anyhow::bail!(
            "database left at v{version}, expected v{LATEST_VERSION} — \
             missing migration step(s)"
        );
    }

    Ok(version)
}
