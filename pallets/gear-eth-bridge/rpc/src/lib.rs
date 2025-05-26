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

use jsonrpsee::{core::RpcResult, proc_macros::rpc, types::error::ErrorObject};
use pallet_gear_eth_bridge_rpc_runtime_api::Proof;
use sp_api::ProvideRuntimeApi;
use sp_blockchain::HeaderBackend;
use sp_core::H256;
use sp_runtime::traits::Block;
use std::sync::Arc;

pub use pallet_gear_eth_bridge_rpc_runtime_api::GearEthBridgeApi as GearEthBridgeRuntimeApi;

#[rpc(server)]
pub trait GearEthBridgeApi<BlockHash> {
    #[method(name = "gearEthBridge_merkleProof")]
    fn merkle_proof(&self, hash: H256, at: Option<BlockHash>) -> RpcResult<Proof>;
}

/// Provides RPC methods to query token economics related data.
pub struct GearEthBridge<C, P> {
    /// Shared reference to the client.
    client: Arc<C>,
    _marker: std::marker::PhantomData<P>,
}

impl<C, P> GearEthBridge<C, P> {
    /// Creates a new instance of the GearBridge Rpc helper.
    pub fn new(client: Arc<C>) -> Self {
        Self {
            client,
            _marker: Default::default(),
        }
    }
}

impl<C, B: Block> GearEthBridgeApiServer<B::Hash> for GearEthBridge<C, B>
where
    C: 'static + ProvideRuntimeApi<B> + HeaderBackend<B>,
    C::Api: GearEthBridgeRuntimeApi<B>,
{
    fn merkle_proof(&self, hash: H256, at: Option<B::Hash>) -> RpcResult<Proof> {
        let api = self.client.runtime_api();
        let at_hash = at.unwrap_or_else(|| self.client.info().best_hash);

        api.merkle_proof(at_hash, hash)
            .map_err(|e| ErrorObject::owned(8000, "RPC error", Some(format!("{e:?}"))))
            .and_then(|opt| {
                opt.ok_or_else(|| {
                    ErrorObject::owned(
                        8000,
                        "Runtime error",
                        Some(String::from("Hash wasn't found in a queue")),
                    )
                })
            })
    }
}
