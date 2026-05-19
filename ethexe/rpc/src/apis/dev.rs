// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use ethexe_common::Address;
#[cfg(feature = "server")]
use ethexe_common::db::ConfigStorageRO;
#[cfg(feature = "server")]
use ethexe_db::Database;
#[cfg(feature = "server")]
use jsonrpsee::core::async_trait;
use jsonrpsee::proc_macros::rpc;

#[cfg_attr(all(feature = "server", feature = "client"), rpc(server, client))]
#[cfg_attr(all(feature = "server", not(feature = "client")), rpc(server))]
#[cfg_attr(all(not(feature = "server"), feature = "client"), rpc(client))]
pub trait Dev {
    /// This call is infallible and always return the protocol Router address.
    #[method(name = "routerAddress")]
    async fn router_address(&self) -> jsonrpsee::core::RpcResult<Address>;
}

#[cfg(feature = "server")]
pub struct DevApi {
    db: Database,
}

#[cfg(feature = "server")]
impl DevApi {
    pub fn new(db: Database) -> Self {
        Self { db }
    }
}

#[cfg(feature = "server")]
#[async_trait]
impl DevServer for DevApi {
    async fn router_address(&self) -> jsonrpsee::core::RpcResult<Address> {
        Ok(self.db.config().router_address)
    }
}
