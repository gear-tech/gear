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

use self::migration::Migration;
use crate::dump::StateDump;
#[cfg(feature = "mock")]
use crate::{Database, MemDb, RawDatabase};
use futures::future::BoxFuture;
use gear_core::code::{CodeMetadata, InstrumentedCode};
use gprimitives::CodeId;
use gsigner::Address;

pub use init::initialize_db;

mod init;
mod migration;

mod v0;
mod v1;
mod v2;
mod v3;
mod v4;

pub const OLDEST_SUPPORTED_VERSION: u32 = v0::VERSION;
pub const LATEST_VERSION: u32 = v4::VERSION;

pub const MIGRATIONS: &[&dyn Migration] = &[
    &v1::migration_from_v0,
    &v2::migration_from_v1,
    &v3::migration_from_v2,
    &v4::migration_from_v3,
];

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

// Some utils functions for database migrations.
pub mod utils {
    use gprimitives::H256;

    const DB_CONFIG_KEY_PREF: u64 = 15;
    const CONFIG_KEY_LEN: usize = size_of::<H256>() + 8;

    pub fn config_key_bytes() -> [u8; CONFIG_KEY_LEN] {
        let mut bytes = [0u8; CONFIG_KEY_LEN];
        let prefix = H256::from_low_u64_be(DB_CONFIG_KEY_PREF);
        bytes[..size_of::<H256>()].copy_from_slice(prefix.as_bytes());
        bytes
    }
}
