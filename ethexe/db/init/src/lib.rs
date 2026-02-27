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

use crate::migration::Migration;
#[cfg(feature = "mock")]
use ethexe_db::{Database, DatabaseRef, MemDb};
use gsigner::Address;
pub use init::{initialize_db, initialize_empty_db};

mod init;
mod migration;

mod v0;
mod v1;

pub const LATEST_VERSION: u32 = v1::VERSION;
pub const MIGRATIONS: &[&dyn for<'c> Migration<'c>] = &[&v1::migration_from_v0];

const _: () = assert!(
    LATEST_VERSION as usize == MIGRATIONS.len(),
    "Wrong number of migrations available"
);

pub struct InitConfig {
    pub ethereum_rpc: String,
    pub router_address: Address,
    pub slot_duration_secs: u64,
}

#[cfg(feature = "mock")]
pub async fn create_initialized_empty_memory_db(config: InitConfig) -> anyhow::Result<Database> {
    let db = MemDb::default();
    initialize_empty_db(config, DatabaseRef { kv: &db, cas: &db }).await?;
    Database::from_one(&db)
}
