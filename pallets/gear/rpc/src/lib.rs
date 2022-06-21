// This file is part of Gear.

// Copyright (C) 2021-2022 Gear Technologies Inc.
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

#![allow(clippy::too_many_arguments)]

use gear_common::Origin;
use gear_core::ids::{MessageId, ProgramId};
use jsonrpsee::{
    core::{async_trait, Error as JsonRpseeError, RpcResult},
    proc_macros::rpc,
    types::error::{CallError, ErrorObject},
};
pub use pallet_gear_rpc_runtime_api::GearApi as GearRuntimeApi;
use pallet_gear_rpc_runtime_api::{GasInfo, HandleKind};
use sp_api::{ApiError, ApiRef, ProvideRuntimeApi};
use sp_blockchain::HeaderBackend;
use sp_core::{Bytes, H256};
use sp_runtime::{generic::BlockId, traits::Block as BlockT};
use std::sync::Arc;

/// Converts a runtime trap into a [`CallError`].
fn runtime_error_into_rpc_error(err: impl std::fmt::Debug) -> JsonRpseeError {
    CallError::Custom(ErrorObject::owned(
        8000,
        "Runtime error",
        Some(format!("{:?}", err)),
    ))
    .into()
}

#[rpc(client, server)]
pub trait GearApi<BlockHash, ResponseType> {
    #[method(name = "gear_calculateInitGas")]
    fn get_init_gas_spent(
        &self,
        source: H256,
        code: Bytes,
        payload: Bytes,
        value: u128,
        allow_other_panics: bool,
        at: Option<BlockHash>,
    ) -> RpcResult<GasInfo>;

    #[method(name = "gear_calculateHandleGas")]
    fn get_handle_gas_spent(
        &self,
        source: H256,
        dest: H256,
        payload: Bytes,
        value: u128,
        allow_other_panics: bool,
        at: Option<BlockHash>,
    ) -> RpcResult<GasInfo>;

    #[method(name = "gear_calculateReplyGas")]
    fn get_reply_gas_spent(
        &self,
        source: H256,
        message_id: H256,
        exit_code: i32,
        payload: Bytes,
        value: u128,
        allow_other_panics: bool,
        at: Option<BlockHash>,
    ) -> RpcResult<GasInfo>;
}

/// A struct that implements the [`GearApi`].
pub struct Gear<C, P> {
    // If you have more generics, no need to Gear<C, M, N, P, ...>
    // just use a tuple like Gear<C, (M, N, P, ...)>
    client: Arc<C>,
    _marker: std::marker::PhantomData<P>,
}

impl<C, P> Gear<C, P> {
    /// Creates a new instance of the Gear Rpc helper.
    pub fn new(client: Arc<C>) -> Self {
        Self {
            client,
            _marker: Default::default(),
        }
    }
}

impl<Client, Block> Gear<Client, Block>
where
    Block: BlockT,
    Client: 'static + ProvideRuntimeApi<Block>,
    Client::Api: GearRuntimeApi<Block>,
{
    fn run_with_api_copy<R, F>(&self, f: F) -> RpcResult<R>
    where
        F: FnOnce(
            ApiRef<<Client as ProvideRuntimeApi<Block>>::Api>,
        ) -> Result<Result<R, Vec<u8>>, ApiError>,
    {
        let api = self.client.runtime_api();

        let runtime_api_result = f(api).map_err(runtime_error_into_rpc_error)?;

        runtime_api_result.map_err(|e| runtime_error_into_rpc_error(String::from_utf8_lossy(&e)))
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

#[async_trait]
impl<C, Block> GearApiServer<<Block as BlockT>::Hash, Result<u64, Vec<u8>>> for Gear<C, Block>
where
    Block: BlockT,
    C: 'static + ProvideRuntimeApi<Block> + HeaderBackend<Block>,
    C::Api: GearRuntimeApi<Block>,
{
    fn get_init_gas_spent(
        &self,
        source: H256,
        code: Bytes,
        payload: Bytes,
        value: u128,
        allow_other_panics: bool,
        at: Option<<Block as BlockT>::Hash>,
    ) -> RpcResult<GasInfo> {
        let at = BlockId::hash(at.unwrap_or_else(||
            // If the block hash is not supplied assume the best block.
            self.client.info().best_hash));

        let GasInfo { min_limit, .. } = self.run_with_api_copy(|api| {
            api.calculate_gas_info(
                &at,
                source,
                HandleKind::Init(code.to_vec()),
                payload.to_vec(),
                value,
                allow_other_panics,
                None,
            )
        })?;
        self.run_with_api_copy(|api| {
            api.calculate_gas_info(
                &at,
                source,
                HandleKind::Init(code.to_vec()),
                payload.to_vec(),
                value,
                allow_other_panics,
                Some(min_limit),
            )
        })
    }

    fn get_handle_gas_spent(
        &self,
        source: H256,
        dest: H256,
        payload: Bytes,
        value: u128,
        allow_other_panics: bool,
        at: Option<<Block as BlockT>::Hash>,
    ) -> RpcResult<GasInfo> {
        let at = BlockId::hash(at.unwrap_or_else(||
            // If the block hash is not supplied assume the best block.
            self.client.info().best_hash));

        let GasInfo { min_limit, .. } = self.run_with_api_copy(|api| {
            api.calculate_gas_info(
                &at,
                source,
                HandleKind::Handle(ProgramId::from_origin(dest)),
                payload.to_vec(),
                value,
                allow_other_panics,
                None,
            )
        })?;
        self.run_with_api_copy(|api| {
            api.calculate_gas_info(
                &at,
                source,
                HandleKind::Handle(ProgramId::from_origin(dest)),
                payload.to_vec(),
                value,
                allow_other_panics,
                Some(min_limit),
            )
        })
    }

    fn get_reply_gas_spent(
        &self,
        source: H256,
        message_id: H256,
        exit_code: i32,
        payload: Bytes,
        value: u128,
        allow_other_panics: bool,
        at: Option<<Block as BlockT>::Hash>,
    ) -> RpcResult<GasInfo> {
        let at = BlockId::hash(at.unwrap_or_else(||
            // If the block hash is not supplied assume the best block.
            self.client.info().best_hash));

        let GasInfo { min_limit, .. } = self.run_with_api_copy(|api| {
            api.calculate_gas_info(
                &at,
                source,
                HandleKind::Reply(MessageId::from_origin(message_id), exit_code),
                payload.to_vec(),
                value,
                allow_other_panics,
                None,
            )
        })?;
        self.run_with_api_copy(|api| {
            api.calculate_gas_info(
                &at,
                source,
                HandleKind::Reply(MessageId::from_origin(message_id), exit_code),
                payload.to_vec(),
                value,
                allow_other_panics,
                Some(min_limit),
            )
        })
    }
}
