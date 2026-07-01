// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use ethexe_common::Address;
use jsonrpsee::proc_macros::rpc;

#[rpc(client)]
pub trait Dev {
    /// This call is infallible and always return the protocol Router address.
    #[method(name = "routerAddress")]
    async fn router_address(&self) -> jsonrpsee::core::RpcResult<Address>;
}
