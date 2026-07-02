// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use jsonrpsee::proc_macros::rpc;

pub const RPC_VERSION: &str = env!("CARGO_PKG_VERSION");

#[rpc(client)]
pub trait Info {
    #[method(name = "version")]
    async fn version(&self) -> jsonrpsee::core::RpcResult<String>;
}
