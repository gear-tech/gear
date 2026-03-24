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

use anyhow::Context as _;
use ethexe_db::{Database, RawDatabase};
use gsigner::Address;

mod version1;

pub const DB_VERSION_0: u32 = 0;
pub const DB_VERSION_1: u32 = 1;

pub struct InitConfig {
    pub ethereum_rpc: String,
    pub router_address: Address,
    pub slot_duration_secs: u64,
}

pub async fn initialize_db(config: InitConfig, raw: RawDatabase) -> anyhow::Result<Database> {
    const _: () = assert!(ethexe_db::VERSION == DB_VERSION_1, "Versions mismatch");

    version1::initialize_db(config, raw.clone()).await?;
    Database::try_from_raw(raw).context("Failed to create database from raw after initialization")
}

#[cfg(feature = "mock")]
pub async fn create_initialized_empty_memory_db(config: InitConfig) -> anyhow::Result<Database> {
    const _: () = assert!(ethexe_db::VERSION == DB_VERSION_1, "Versions mismatch");

    let raw = RawDatabase::from_one(&ethexe_db::MemDb::default());
    version1::initialize_empty_db(config, raw.clone()).await?;
    Database::try_from_raw(raw)
}
