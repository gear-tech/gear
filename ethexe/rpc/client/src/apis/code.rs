// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use gprimitives::H256;
use jsonrpsee::proc_macros::rpc;
use sp_core::Bytes;

#[rpc(client)]
pub trait Code {
    #[method(name = "code_getOriginal")]
    async fn get_original_code(&self, id: H256) -> jsonrpsee::core::RpcResult<Bytes>;

    #[method(name = "code_getInstrumented")]
    async fn get_instrumented_code(
        &self,
        runtime_id: u32,
        code_id: H256,
    ) -> jsonrpsee::core::RpcResult<Bytes>;

    #[method(name = "code_readWasmCustomSection")]
    async fn read_wasm_custom_section(
        &self,
        code_id: H256,
        section_name: String,
    ) -> jsonrpsee::core::RpcResult<Option<Bytes>>;
}
