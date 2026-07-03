// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use ethexe_common::{Address, db::ConfigStorageRO};
use ethexe_db::Database;
use jsonrpsee::{core::async_trait, proc_macros::rpc};

#[rpc(server)]
pub trait Dev {
    /// This call is infallible and always return the protocol Router address.
    #[method(name = "routerAddress")]
    async fn router_address(&self) -> jsonrpsee::core::RpcResult<Address>;
}

pub struct DevApi {
    db: Database,
}

impl DevApi {
    pub fn new(db: Database) -> Self {
        Self { db }
    }
}

#[async_trait]
impl DevServer for DevApi {
    async fn router_address(&self) -> jsonrpsee::core::RpcResult<Address> {
        Ok(self.db.config().router_address)
    }
}
