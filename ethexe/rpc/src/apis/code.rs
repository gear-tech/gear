// This file is part of Gear.
//
// Copyright (C) 2024-2025 Gear Technologies Inc.
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

use crate::errors;
use ethexe_db::{CodesStorage, Database};
use gprimitives::H256;
use jsonrpsee::{
    core::{async_trait, RpcResult},
    proc_macros::rpc,
};
use parity_scale_codec::Encode;
use sp_core::Bytes;

#[rpc(server)]
pub trait Code {
    #[method(name = "code_get")]
    async fn get_code(&self, id: H256) -> RpcResult<Bytes>;
}

pub struct CodeApi {
    db: Database,
}

impl CodeApi {
    pub fn new(db: Database) -> Self {
        Self { db }
    }
}

#[async_trait]
impl CodeServer for CodeApi {
    async fn get_code(&self, id: H256) -> RpcResult<Bytes> {
        self.db
            .original_code(id.into())
            .map(|bytes| bytes.encode().into())
            .ok_or_else(|| errors::db("Failed to get code by supplied id"))
    }
}
