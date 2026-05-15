// This file is part of Gear.
//
// Copyright (C) 2024-2025 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use ethexe_common::Address;
#[cfg(feature = "server")]
use ethexe_common::db::ConfigStorageRO;
#[cfg(feature = "server")]
use ethexe_db::Database;
use jsonrpsee::proc_macros::rpc;
#[cfg(feature = "server")]
use jsonrpsee::core::async_trait;

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
