// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use crate::errors;
use ethexe_common::db::CodesStorageRO;
use ethexe_db::Database;
use gprimitives::H256;
use jsonrpsee::{
    core::{RpcResult, async_trait},
    proc_macros::rpc,
};
use parity_scale_codec::Encode;
use sp_core::Bytes;

#[cfg_attr(not(feature = "client"), rpc(server))]
#[cfg_attr(feature = "client", rpc(server, client))]
pub trait Code {
    #[method(name = "code_getOriginal")]
    async fn get_original_code(&self, id: H256) -> RpcResult<Bytes>;

    #[method(name = "code_getInstrumented")]
    async fn get_instrumented_code(&self, runtime_id: u32, code_id: H256) -> RpcResult<Bytes>;
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
    async fn get_original_code(&self, id: H256) -> RpcResult<Bytes> {
        self.db
            .original_code(id.into())
            .map(|bytes| bytes.encode().into())
            .ok_or_else(|| errors::db("Failed to get code by supplied id"))
    }

    async fn get_instrumented_code(&self, runtime_id: u32, code_id: H256) -> RpcResult<Bytes> {
        self.db
            .instrumented_code(runtime_id, code_id.into())
            .map(|bytes| bytes.encode().into())
            .ok_or_else(|| errors::db("Failed to get code by supplied id"))
    }
}
