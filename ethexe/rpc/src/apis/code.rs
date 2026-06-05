// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

#[cfg(feature = "server")]
use crate::errors;
#[cfg(feature = "server")]
use ethexe_common::db::CodesStorageRO;
#[cfg(feature = "server")]
use ethexe_db::Database;
#[cfg(feature = "server")]
use gear_core::code::get_custom_section_data;
use gprimitives::H256;
#[cfg(feature = "server")]
use jsonrpsee::core::async_trait;
use jsonrpsee::proc_macros::rpc;
#[cfg(feature = "server")]
use parity_scale_codec::Encode;
use sp_core::Bytes;

#[cfg_attr(all(feature = "server", feature = "client"), rpc(server, client))]
#[cfg_attr(all(feature = "server", not(feature = "client")), rpc(server))]
#[cfg_attr(all(not(feature = "server"), feature = "client"), rpc(client))]
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

#[cfg(feature = "server")]
pub struct CodeApi {
    db: Database,
}

#[cfg(feature = "server")]
impl CodeApi {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    fn read_custom_section(
        &self,
        code_id: H256,
        section_name: &str,
    ) -> jsonrpsee::core::RpcResult<Option<Bytes>> {
        let Some(original_code) = self.db.original_code(code_id.into()) else {
            return Ok(None);
        };

        get_custom_section_data(&original_code, section_name)
            .map(|section| section.map(|section| Bytes(section.to_vec())))
            .map_err(|err| errors::bad_request(err).into())
    }
}

#[cfg(feature = "server")]
#[async_trait]
impl CodeServer for CodeApi {
    async fn get_original_code(&self, id: H256) -> jsonrpsee::core::RpcResult<Bytes> {
        self.db
            .original_code(id.into())
            .map(|bytes| bytes.encode().into())
            .ok_or_else(|| errors::db("Failed to get code by supplied id"))
    }

    async fn get_instrumented_code(
        &self,
        runtime_id: u32,
        code_id: H256,
    ) -> jsonrpsee::core::RpcResult<Bytes> {
        self.db
            .instrumented_code(runtime_id, code_id.into())
            .map(|bytes| bytes.encode().into())
            .ok_or_else(|| errors::db("Failed to get code by supplied id"))
    }

    async fn read_wasm_custom_section(
        &self,
        code_id: H256,
        section_name: String,
    ) -> jsonrpsee::core::RpcResult<Option<Bytes>> {
        self.read_custom_section(code_id, &section_name)
    }
}

#[cfg(all(test, feature = "server"))]
mod tests {
    use super::*;
    use crate::test_utils::{wasm_with_custom_section, wasm_with_custom_sections};
    use ethexe_common::db::CodesStorageRW;

    const SECTION_NAME: &str = "sails:idl";
    const SECTION_DATA: &[u8] = b"hello idl";

    fn api_with_code(code: &[u8]) -> (CodeApi, H256) {
        let db = Database::memory();
        let code_id = H256::from(db.set_original_code(code).into_bytes());

        (CodeApi::new(db), code_id)
    }

    #[tokio::test]
    async fn read_wasm_custom_section_returns_found_section() {
        let wasm = wasm_with_custom_section(SECTION_NAME, SECTION_DATA);
        let (api, code_id) = api_with_code(&wasm);

        let result = api
            .read_custom_section(code_id, SECTION_NAME)
            .expect("custom section read must succeed");

        assert_eq!(result, Some(Bytes(SECTION_DATA.to_vec())));
    }

    #[tokio::test]
    async fn read_wasm_custom_section_returns_first_matching_section() {
        let wasm =
            wasm_with_custom_sections(&[(SECTION_NAME, b"first"), (SECTION_NAME, b"second")]);
        let (api, code_id) = api_with_code(&wasm);

        let result = api
            .read_custom_section(code_id, SECTION_NAME)
            .expect("custom section read must succeed");

        assert_eq!(result, Some(Bytes(b"first".to_vec())));
    }

    #[tokio::test]
    async fn read_wasm_custom_section_returns_none_for_missing_section() {
        let wasm = wasm_with_custom_section("other", b"data");
        let (api, code_id) = api_with_code(&wasm);

        let result = api
            .read_custom_section(code_id, SECTION_NAME)
            .expect("custom section read must succeed");

        assert_eq!(result, None);
    }

    #[tokio::test]
    async fn read_wasm_custom_section_returns_none_for_unknown_code() {
        let api = CodeApi::new(Database::memory());

        let result = api
            .read_custom_section(H256::zero(), SECTION_NAME)
            .expect("unknown code must not be an error");

        assert_eq!(result, None);
    }

    #[tokio::test]
    async fn read_wasm_custom_section_errors_for_malformed_stored_wasm() {
        let mut wasm = wasm_with_custom_section(SECTION_NAME, SECTION_DATA);
        wasm.extend_from_slice(b"trailing junk");
        let (api, code_id) = api_with_code(&wasm);

        let err = api
            .read_custom_section(code_id, SECTION_NAME)
            .expect_err("malformed stored wasm must be an RPC error");

        assert_eq!(err.code(), 8000);
    }
}
