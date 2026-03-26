// This file is part of Gear.
//
// Copyright (C) 2026 Gear Technologies Inc.
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

use crate::{errors, utils};
use ethexe_common::{Announce, ProgramStates, db::AnnounceStorageRO, gear::StateTransition};
use ethexe_db::Database;
use jsonrpsee::{
    core::{RpcResult, async_trait},
    proc_macros::rpc,
};
use sp_core::H256;

#[cfg_attr(not(feature = "client"), rpc(server))]
#[cfg_attr(feature = "client", rpc(server, client))]
pub trait Announce {
    #[method(name = "announce_by_hash")]
    async fn announce(&self, announce_hash: Option<H256>) -> RpcResult<(H256, Announce)>;

    #[method(name = "announce_outcome")]
    async fn announce_outcome(
        &self,
        announce_hash: Option<H256>,
    ) -> RpcResult<Vec<StateTransition>>;

    #[method(name = "announce_program_states")]
    async fn announce_program_states(
        &self,
        announce_hash: Option<H256>,
    ) -> RpcResult<ProgramStates>;
}

#[derive(Debug, Clone)]
pub struct AnnounceApi {
    db: Database,
}

impl AnnounceApi {
    pub fn new(db: Database) -> Self {
        Self { db }
    }
}

#[async_trait]
impl AnnounceServer for AnnounceApi {
    async fn announce(&self, announce_hash: Option<H256>) -> RpcResult<(H256, Announce)> {
        let hash = utils::announce_at_or_latest_computed(&self.db, announce_hash)?;
        let announce = self
            .db
            .announce(hash)
            .ok_or_else(|| errors::db("Announce wasn't found"))?;

        Ok((hash.inner(), announce))
    }

    async fn announce_outcome(
        &self,
        announce_hash: Option<H256>,
    ) -> RpcResult<Vec<StateTransition>> {
        let hash = utils::announce_at_or_latest_computed(&self.db, announce_hash)?;
        self.db
            .announce_outcome(hash)
            .ok_or_else(|| errors::db("Announce outcome wasn't found"))
    }

    async fn announce_program_states(
        &self,
        announce_hash: Option<H256>,
    ) -> RpcResult<ProgramStates> {
        let hash = utils::announce_at_or_latest_computed(&self.db, announce_hash)?;
        self.db
            .announce_program_states(hash)
            .ok_or_else(|| errors::db("Announce program states weren't found"))
    }
}
