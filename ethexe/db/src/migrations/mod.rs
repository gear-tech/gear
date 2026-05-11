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

use crate::dump::StateDump;
#[cfg(feature = "mock")]
use crate::{Database, MemDb, RawDatabase};
use futures::future::BoxFuture;
use gear_core::code::{CodeMetadata, InstrumentedCode};
use gprimitives::CodeId;
use gsigner::Address;

pub use init::initialize_db;

mod init;

/// Single supported on-disk schema version; databases below this version
/// must be wiped and re-initialised.
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

// +_+_+ return back migrations logic, but without v1, v2, ... migrations implementations, they are useless for now.
