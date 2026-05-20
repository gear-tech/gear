// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

#[cfg(feature = "server")]
use crate::errors;
#[cfg(feature = "server")]
use ethexe_common::db::CodesStorageRO;
#[cfg(feature = "server")]
use ethexe_db::Database;
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
}
