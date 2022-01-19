// This file is part of Gear.

// Copyright (C) 2021 Gear Technologies Inc.
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

use codec::Codec;
use jsonrpc_core::{Error as RpcError, ErrorCode, Result};
use jsonrpc_derive::rpc;
pub use pallet_gear_rpc_runtime_api::GearApi as GearRuntimeApi;
use sp_api::ProvideRuntimeApi;
use sp_blockchain::HeaderBackend;
use sp_core::Bytes;
use sp_rpc::number::NumberOrHex;
use sp_runtime::{generic::BlockId, traits::Block as BlockT};
use std::convert::TryInto;
use std::sync::Arc;

#[rpc]
pub trait GearApi<BlockHash, AccountId, ProgramId> {
    #[rpc(name = "gear_getGasSpent")]
    fn get_gas_spent(
        &self,
        account_id: AccountId,
        program_id: ProgramId,
        payload: Bytes,
        at: Option<BlockHash>,
    ) -> Result<NumberOrHex>;
}

/// A struct that implements the [`GearApi`].
pub struct Gear<C, P> {
    // If you have more generics, no need to Gear<C, M, N, P, ...>
    // just use a tuple like Gear<C, (M, N, P, ...)>
    client: Arc<C>,
    _marker: std::marker::PhantomData<P>,
}

impl<C, P> Gear<C, P> {
    /// Create new `Gear` instance with the given reference to the client.
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

impl From<Error> for i64 {
    fn from(e: Error) -> i64 {
        match e {
            Error::RuntimeError => 1,
            Error::DecodeError => 2,
        }
    }
}

impl<C, Block, AccountId, ProgramId> GearApi<<Block as BlockT>::Hash, AccountId, ProgramId>
    for Gear<C, Block>
where
    Block: BlockT,
    C: 'static + ProvideRuntimeApi<Block> + HeaderBackend<Block>,
    C::Api: GearRuntimeApi<Block, AccountId, ProgramId>,
    AccountId: Codec,
    ProgramId: Codec + Copy,
{
    fn get_gas_spent(
        &self,
        account_id: AccountId,
        program_id: ProgramId,
        payload: Bytes,
        at: Option<<Block as BlockT>::Hash>,
    ) -> Result<NumberOrHex> {
        let api = self.client.runtime_api();
        let at = BlockId::hash(at.unwrap_or_else(||
			// If the block hash is not supplied assume the best block.
			self.client.info().best_hash));

        let runtime_api_result = api
            .get_gas_spent(&at, account_id, program_id, payload.to_vec())
            .map_err(|e| RpcError {
                code: ErrorCode::ServerError(Error::RuntimeError.into()),
                message: "Unable to get gas spent.".into(),
                data: Some(format!("{:?}", e).into()),
            })?;

        let try_into_rpc_gas_spent = |value: u64| {
            value.try_into().map_err(|_| RpcError {
                code: ErrorCode::InvalidParams,
                message: format!("{} doesn't fit in NumberOrHex representation", value),
                data: None,
            })
        };

        match runtime_api_result {
            Some(value) => Ok(try_into_rpc_gas_spent(value)?),
            None => Err(RpcError {
                code: ErrorCode::ServerError(Error::RuntimeError.into()),
                message: "Empty run result".to_string(),
                data: None,
            }),
        }
    }
}
