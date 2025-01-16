// This file is part of Gear.
//
// Copyright (C) 2024 Gear Technologies Inc.
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

use std::sync::Arc;

use ethexe_observer::MockBlobReader;
use gear_core::ids::prelude::CodeIdExt;
use gprimitives::{CodeId, H256};
use jsonrpsee::{
    core::{async_trait, RpcResult},
    proc_macros::rpc,
};
use sp_core::Bytes;

#[rpc(server)]
pub trait Dev {
    #[method(name = "dev_setBlob")]
    async fn set_blob(&self, blob: Bytes) -> RpcResult<(H256, CodeId)>;
}

#[derive(Clone)]
pub struct DevApi {
    blob_reader: Arc<MockBlobReader>,
}

impl DevApi {
    pub fn new(blob_reader: Arc<MockBlobReader>) -> Self {
        Self { blob_reader }
    }
}

#[async_trait]
impl DevServer for DevApi {
    async fn set_blob(&self, blob: Bytes) -> RpcResult<(H256, CodeId)> {
        let code_id = CodeId::generate(&blob);
        let blob_tx = H256::random();

        self.blob_reader.add_blob_transaction(blob_tx, blob.0).await;

        Ok((blob_tx, code_id))
    }
}