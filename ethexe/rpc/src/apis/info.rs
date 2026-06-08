// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

#[cfg(feature = "server")]
use jsonrpsee::{core::async_trait, proc_macros::rpc};

pub const RPC_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(all(feature = "server", feature = "client"), rpc(server, client))]
#[cfg_attr(all(feature = "server", not(feature = "client")), rpc(server))]
#[cfg_attr(all(not(feature = "server"), feature = "client"), rpc(client))]
pub trait Info {
    #[method(name = "version")]
    async fn version(&self) -> jsonrpsee::core::RpcResult<String>;
}

#[cfg(feature = "server")]
pub struct InfoApi;

#[cfg(feature = "server")]
#[async_trait]
impl InfoServer for InfoApi {
    async fn version(&self) -> jsonrpsee::core::RpcResult<String> {
        Ok(RPC_VERSION.into())
    }
}
