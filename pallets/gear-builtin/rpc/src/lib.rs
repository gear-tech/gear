// This file is part of Gear.

// Copyright (C) 2021-2024 Gear Technologies Inc.
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
    core::{Error as JsonRpseeError, RpcResult},
    proc_macros::rpc,
    types::error::{CallError, ErrorObject},
};
pub use pallet_gear_builtin_rpc_runtime_api::GearBuiltinApi as GearBuiltinRuntimeApi;
use sp_api::ProvideRuntimeApi;
use sp_blockchain::HeaderBackend;
use sp_core::H256;
use sp_runtime::traits::Block as BlockT;
use std::sync::Arc;

#[rpc(server)]
pub trait GearBuiltinApi<BlockHash, ResponseType> {
    #[method(name = "gearBuiltin_queryId")]
    fn query_actor_id(&self, builtin_id: u64) -> RpcResult<ResponseType>;
}

/// Provides RPC methods to query token economics related data.
pub struct GearBuiltin<C, P> {
    /// Shared reference to the client.
    client: Arc<C>,
    _marker: std::marker::PhantomData<P>,
}

impl<C, P> GearBuiltin<C, P> {
    /// Creates a new instance of the GearBuiltin Rpc helper.
    pub fn new(client: Arc<C>) -> Self {
        Self {
            client,
            _marker: Default::default(),
        }
    }
}

/// Error type of this RPC api.
pub enum Error {
    /// The query was not decodable.
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

impl<C, Block> GearBuiltinApiServer<<Block as BlockT>::Hash, H256> for GearBuiltin<C, Block>
where
    Block: BlockT,
    C: 'static + ProvideRuntimeApi<Block> + HeaderBackend<Block>,
    C::Api: GearBuiltinRuntimeApi<Block>,
{
    fn query_actor_id(&self, builtin_id: u64) -> RpcResult<H256> {
        let api = self.client.runtime_api();
        let best_hash = self.client.info().best_hash;

        fn map_err(err: impl std::fmt::Debug, desc: &'static str) -> JsonRpseeError {
            CallError::Custom(ErrorObject::owned(8000, desc, Some(format!("{err:?}")))).into()
        }

        api.query_actor_id(best_hash, builtin_id)
            .map_err(|e| map_err(e, "Unable to generate actor id"))
    }
}
