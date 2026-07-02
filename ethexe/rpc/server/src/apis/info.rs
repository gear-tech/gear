// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use jsonrpsee::{core::async_trait, proc_macros::rpc};

pub const RPC_VERSION: &str = env!("CARGO_PKG_VERSION");

#[rpc(server)]
pub trait Info {
    #[method(name = "version")]
    async fn version(&self) -> jsonrpsee::core::RpcResult<String>;
}

pub struct InfoApi;

#[async_trait]
impl InfoServer for InfoApi {
    async fn version(&self) -> jsonrpsee::core::RpcResult<String> {
        Ok(RPC_VERSION.into())
    }
}
