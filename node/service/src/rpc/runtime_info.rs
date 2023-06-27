// This file is part of Gear.

// Copyright (C) 2021-2023 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! This module is used to add custom runtime irrelevant RPC endpoints to the node.

use jsonrpsee::{
    core::{Error as RpcError, RpcResult},
    proc_macros::rpc,
};
use sc_client_api::StorageProvider;
use sc_executor_common::runtime_blob::RuntimeBlob;
use sp_blockchain::HeaderBackend;
use sp_runtime::traits::Block as BlockT;
use std::{marker::PhantomData, sync::Arc};

#[rpc(server)]
pub(crate) trait RuntimeInfo<BlockHash> {
    // Returns the version of the WASM blob in storage.
    // The format of the version is `x.y.z-commit_hash`, where the `x.y.z` is the version
    // of the runtime crate, and the `commit_hash` is the hash of the commit the runtime crate
    // was built from.
    #[method(name = "runtime_wasmBlobVersion")]
    fn wasm_blob_version(&self, at: Option<BlockHash>) -> RpcResult<String>;
}

pub(crate) struct RuntimeInfoApi<C, Block, Backend> {
    client: Arc<C>,
    _marker1: PhantomData<Block>,
    _marker2: PhantomData<Backend>,
}

impl<C, Block, Backend> RuntimeInfoApi<C, Block, Backend> {
    pub(crate) fn new(client: Arc<C>) -> Self {
        Self {
            client,
            _marker1: PhantomData,
            _marker2: PhantomData,
        }
    }
}

impl<C, Block, Backend> RuntimeInfoServer<<Block as BlockT>::Hash>
    for RuntimeInfoApi<C, Block, Backend>
where
    C: HeaderBackend<Block> + StorageProvider<Block, Backend> + Send + Sync + 'static,
    Block: BlockT,
    Backend: sc_client_api::Backend<Block> + Send + Sync + 'static,
{
    fn wasm_blob_version(&self, at: Option<Block::Hash>) -> RpcResult<String> {
        let at = at.unwrap_or_else(|| self.client.info().best_hash);

        let wasm_blob_data = self
            .client
            .storage(
                at,
                &sp_storage::StorageKey(sp_core::storage::well_known_keys::CODE.into()),
            )
            .map_err(map_err_into_rpc_err)?;
        let Some(wasm_blob_data) = wasm_blob_data else { return Err(rpc_err("Unable to find WASM blob in storage", None)); };

        let wasm_runtime_blob =
            RuntimeBlob::uncompress_if_needed(&wasm_blob_data.0).map_err(map_err_into_rpc_err)?;

        let wasm_blob_version = wasm_runtime_blob.custom_section_contents("wasm_blob_version");
        let Some(wasm_blob_version) = wasm_blob_version else { return Err(rpc_err("Unable to find WASM blob version in WASM blob", None)); };
        let wasm_blob_version =
            String::from_utf8(wasm_blob_version.into()).map_err(map_err_into_rpc_err)?;

        Ok(wasm_blob_version)
    }
}

fn map_err_into_rpc_err(err: impl std::fmt::Debug) -> RpcError {
    rpc_err("Runtime info error", Some(format!("{err:?}")))
}

fn rpc_err(message: &str, data: Option<String>) -> RpcError {
    use jsonrpsee::types::error::{CallError, ErrorObject};

    CallError::Custom(ErrorObject::owned(9000, message, data)).into()
}
