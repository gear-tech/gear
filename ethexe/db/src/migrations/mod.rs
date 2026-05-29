// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use self::migration::Migration;
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
mod migration;
mod v1;

pub const LATEST_VERSION: u32 = v1::VERSION;

pub const OLDEST_SUPPORTED_VERSION: u32 = v1::VERSION;

pub const MIGRATIONS: &[&dyn Migration] = &[];

const _: () = assert!(
    (LATEST_VERSION - OLDEST_SUPPORTED_VERSION) as usize == MIGRATIONS.len(),
    "Wrong number of migrations available"
);

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

/// Walk [`MIGRATIONS`] applying any whose `source_version` matches the
/// on-disk database version, until the version reaches
/// [`LATEST_VERSION`]. Errors if a step fails or the resulting version
/// doesn't reach the target.
pub async fn migrate(config: &InitConfig, raw: &RawDatabase) -> Result<u32> {
    let mut version = raw
        .kv
        .version()
        .context("failed to read database version")?
        .context("database has no version key")?;

    if version < OLDEST_SUPPORTED_VERSION {
        anyhow::bail!(
            "database version {version} is older than the oldest supported \
             {OLDEST_SUPPORTED_VERSION}; please wipe the database"
        );
    }
    if version > LATEST_VERSION {
        anyhow::bail!(
            "database version {version} is newer than supported {LATEST_VERSION}; \
             refusing to downgrade"
        );
    }

    for m in MIGRATIONS {
        if version != m.source_version() {
            continue;
        }
        m.migrate(config, raw)
            .await
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
