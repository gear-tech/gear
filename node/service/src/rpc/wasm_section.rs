// This file is part of Gear.

// Copyright (C) 2021-2025 Gear Technologies Inc.
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

//! RPC endpoint for reading custom sections from program WASM original code.

use gear_core::code::get_custom_section_data;
use jsonrpsee::{core::RpcResult, proc_macros::rpc, types::ErrorObjectOwned};
use parity_scale_codec::Decode;
use sc_client_api::StorageProvider;
use sp_blockchain::HeaderBackend;
use sp_core::{Bytes, H256, twox_128};
use sp_runtime::traits::Block as BlockT;
use sp_storage::StorageKey;
use std::{marker::PhantomData, sync::Arc};

#[rpc(server)]
pub(crate) trait WasmSection<BlockHash> {
    /// Read a custom section from the original WASM code stored on-chain.
    ///
    /// This is commonly used to retrieve the Sails IDL embedded in the
    /// `sails:idl` custom section. The returned bytes are raw section data;
    /// for `sails:idl`, clients must parse the envelope (version + flags)
    /// and decompress the payload.
    ///
    /// Returns `null` if the code is not found or the section does not exist.
    #[method(name = "gear_readWasmCustomSection")]
    fn read_wasm_custom_section(
        &self,
        code_id: H256,
        section_name: String,
        at: Option<BlockHash>,
    ) -> RpcResult<Option<Bytes>>;
}

pub(crate) struct WasmSectionApi<C, Block, Backend> {
    client: Arc<C>,
    original_code_prefix: Vec<u8>,
    _marker1: PhantomData<Block>,
    _marker2: PhantomData<Backend>,
}

impl<C, Block, Backend> WasmSectionApi<C, Block, Backend> {
    pub(crate) fn new(client: Arc<C>) -> Self {
        let mut original_code_prefix = twox_128(b"GearProgram").to_vec();
        original_code_prefix.extend_from_slice(&twox_128(b"OriginalCodeStorage"));

        Self {
            client,
            original_code_prefix,
            _marker1: PhantomData,
            _marker2: PhantomData,
        }
    }
}

impl<C, Block, Backend> WasmSectionServer<<Block as BlockT>::Hash>
    for WasmSectionApi<C, Block, Backend>
where
    C: HeaderBackend<Block> + StorageProvider<Block, Backend> + Send + Sync + 'static,
    Block: BlockT,
    Backend: sc_client_api::Backend<Block> + Send + Sync + 'static,
{
    fn read_wasm_custom_section(
        &self,
        code_id: H256,
        section_name: String,
        at: Option<Block::Hash>,
    ) -> RpcResult<Option<Bytes>> {
        let at = at.unwrap_or_else(|| self.client.info().best_hash);

        // Construct storage key: prefix ++ Identity(code_id)
        let mut storage_key = self.original_code_prefix.clone();
        storage_key.extend_from_slice(code_id.as_bytes());

        let wasm_data = self
            .client
            .storage(at, &StorageKey(storage_key))
            .map_err(map_err_into_rpc_err)?;

        let Some(wasm_data) = wasm_data else {
            return Ok(None);
        };

        // The storage value is SCALE-encoded Vec<u8>, so we need to decode it.
        let wasm_bytes: Vec<u8> =
            Decode::decode(&mut wasm_data.0.as_slice())
                .map_err(|e| rpc_err("Failed to decode stored WASM", Some(format!("{e:?}"))))?;

        match get_custom_section_data(&wasm_bytes, &section_name) {
            Ok(Some(data)) => Ok(Some(Bytes(data.to_vec()))),
            Ok(None) => Ok(None),
            Err(e) => Err(rpc_err(
                "Failed to parse stored WASM",
                Some(e.to_string()),
            )),
        }
    }
}

fn map_err_into_rpc_err(err: impl std::fmt::Debug) -> ErrorObjectOwned {
    rpc_err("WASM section read error", Some(format!("{err:?}")))
}

fn rpc_err(message: &str, data: Option<String>) -> ErrorObjectOwned {
    use jsonrpsee::types::error::ErrorObject;

    ErrorObject::owned(9000, message, data)
}
