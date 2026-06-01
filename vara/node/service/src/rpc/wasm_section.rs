// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! RPC endpoint for reading custom sections from program WASM original code.

use gear_core::code::get_custom_section_data;
use jsonrpsee::{core::RpcResult, proc_macros::rpc, types::ErrorObjectOwned};
use parity_scale_codec::{Compact, Decode};
use sc_client_api::StorageProvider;
use sp_blockchain::HeaderBackend;
use sp_core::{Bytes, H256, twox_128};
use sp_runtime::traits::Block as BlockT;
use sp_storage::StorageKey;
use std::{marker::PhantomData, sync::Arc};

const ERROR_CODE: i32 = 9000;

#[rpc(server)]
pub(crate) trait WasmSection<BlockHash> {
    /// Read a custom section from the original WASM code stored on-chain.
    ///
    /// This is commonly used to retrieve the Sails IDL embedded in the
    /// `sails:idl` custom section. The returned bytes are raw section data.
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
    _marker: PhantomData<(Block, Backend)>,
}

impl<C, Block, Backend> WasmSectionApi<C, Block, Backend> {
    pub(crate) fn new(client: Arc<C>) -> Self {
        Self {
            client,
            _marker: PhantomData,
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

        // Keep in sync with `pallet-gear-program::OriginalCodeStorage` (StorageMap, Identity hasher).
        let mut storage_key = twox_128(b"GearProgram").to_vec();
        storage_key.extend_from_slice(&twox_128(b"OriginalCodeStorage"));
        storage_key.extend_from_slice(code_id.as_bytes());

        let wasm_data = self
            .client
            .storage(at, &StorageKey(storage_key))
            .map_err(|e| rpc_err("WASM section read error", Some(e.to_string())))?;

        let Some(wasm_data) = wasm_data else {
            return Ok(None);
        };

        let mut input = wasm_data.0.as_slice();
        let len = <Compact<u32>>::decode(&mut input)
            .map_err(|e| rpc_err("Failed to decode stored WASM length", Some(e.to_string())))?
            .0 as usize;
        if input.len() < len {
            return Err(rpc_err("Truncated stored WASM blob", None));
        }

        match get_custom_section_data(&input[..len], &section_name) {
            Ok(Some(data)) => Ok(Some(Bytes(data.to_vec()))),
            Ok(None) => Ok(None),
            Err(e) => Err(rpc_err("Failed to parse stored WASM", Some(e.to_string()))),
        }
    }
}

fn rpc_err(message: &str, data: Option<String>) -> ErrorObjectOwned {
    use jsonrpsee::types::error::ErrorObject;

    ErrorObject::owned(ERROR_CODE, message, data)
}
