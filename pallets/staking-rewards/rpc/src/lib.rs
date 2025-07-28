// This file is part of Gear.

// Copyright (C) 2021-2025 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! RPC interface for the gear module.

use jsonrpsee::{
    core::RpcResult,
    proc_macros::rpc,
    types::{ErrorObjectOwned, error::ErrorObject},
};
pub use pallet_gear_staking_rewards_rpc_runtime_api::GearStakingRewardsApi as GearStakingRewardsRuntimeApi;
use pallet_gear_staking_rewards_rpc_runtime_api::InflationInfo;
use sp_api::ProvideRuntimeApi;
use sp_blockchain::HeaderBackend;
use sp_runtime::traits::Block as BlockT;
use std::sync::Arc;

#[rpc(server)]
pub trait GearStakingRewardsApi<BlockHash, ResponseType> {
    #[method(name = "stakingRewards_inflationInfo")]
    fn query_inflation_info(&self, at: Option<BlockHash>) -> RpcResult<ResponseType>;
}

/// Provides RPC methods to query token economics related data.
pub struct GearStakingRewards<C, P> {
    /// Shared reference to the client.
    client: Arc<C>,
    _marker: std::marker::PhantomData<P>,
}

impl<C, P> GearStakingRewards<C, P> {
    /// Creates a new instance of the GearStakingRewards Rpc helper.
    pub fn new(client: Arc<C>) -> Self {
        Self {
            client,
            _marker: Default::default(),
        }
    }
}

/// Error type of this RPC api.
pub enum Error {
    /// The transaction was not decodable.
    DecodeError,
    /// The call to runtime failed.
    RuntimeError,
}

impl From<Error> for i32 {
    fn from(e: Error) -> i32 {
        match e {
            Error::RuntimeError => 1,
            Error::DecodeError => 2,
        }
    }
}

impl<C, Block> GearStakingRewardsApiServer<<Block as BlockT>::Hash, InflationInfo>
    for GearStakingRewards<C, Block>
where
    Block: BlockT,
    C: 'static + ProvideRuntimeApi<Block> + HeaderBackend<Block>,
    C::Api: GearStakingRewardsRuntimeApi<Block>,
{
    fn query_inflation_info(&self, at: Option<Block::Hash>) -> RpcResult<InflationInfo> {
        let api = self.client.runtime_api();
        let at_hash = at.unwrap_or_else(|| self.client.info().best_hash);

        fn map_err(err: impl std::fmt::Debug, desc: &'static str) -> ErrorObjectOwned {
            ErrorObject::owned(8000, desc, Some(format!("{err:?}")))
        }

        api.inflation_info(at_hash)
            .map_err(|e| map_err(e, "Unable to query inflation info"))
    }
}
