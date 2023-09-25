// This file is part of Gear.

// Copyright (C) 2021-2023 Gear Technologies Inc.
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
#![allow(where_clauses_object_safety)]

use gear_common::Origin;
use gear_core::ids::{CodeId, MessageId, ProgramId};
use gear_core_errors::*;
use jsonrpsee::{
    core::{async_trait, Error as JsonRpseeError, RpcResult},
    proc_macros::rpc,
    types::error::{CallError, ErrorObject},
};
pub use pallet_gear_rpc_runtime_api::GearApi as GearRuntimeApi;
use pallet_gear_rpc_runtime_api::{GasInfo, HandleKind};
use sp_api::{ApiError, ApiExt, ApiRef, ProvideRuntimeApi};
use sp_blockchain::HeaderBackend;
use sp_core::{Bytes, H256};
use sp_runtime::traits::Block as BlockT;
use std::sync::Arc;

/// Converts a runtime trap into a [`CallError`].
fn runtime_error_into_rpc_error(err: impl std::fmt::Debug) -> JsonRpseeError {
    CallError::Custom(ErrorObject::owned(
        8000,
        "Runtime error",
        Some(format!("{err:?}")),
    ))
    .into()
}

#[rpc(server)]
pub trait GearApi<BlockHash, ResponseType> {
    #[method(name = "gear_calculateInitCreateGas")]
    fn get_init_create_gas_spent(
        &self,
        source: H256,
        code_id: H256,
        payload: Bytes,
        value: u128,
        allow_other_panics: bool,
        at: Option<BlockHash>,
    ) -> RpcResult<GasInfo>;

    #[method(name = "gear_calculateInitUploadGas")]
    fn get_init_upload_gas_spent(
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
        payload: Bytes,
        value: u128,
        allow_other_panics: bool,
        at: Option<BlockHash>,
    ) -> RpcResult<GasInfo>;

    #[method(name = "gear_readState")]
    fn read_state(
        &self,
        program_id: H256,
        payload: Bytes,
        at: Option<BlockHash>,
    ) -> RpcResult<Bytes>;

    #[method(name = "gear_readStateBatch")]
    fn read_state_batch(
        &self,
        batch_id_payload: Vec<(H256, Bytes)>,
        at: Option<BlockHash>,
    ) -> RpcResult<Vec<Bytes>>;

    #[method(name = "gear_readStateUsingWasm")]
    fn read_state_using_wasm(
        &self,
        program_id: H256,
        payload: Bytes,
        fn_name: Bytes,
        wasm: Bytes,
        argument: Option<Bytes>,
        at: Option<BlockHash>,
    ) -> RpcResult<Bytes>;

    #[method(name = "gear_readStateUsingWasmBatch")]
    fn read_state_using_wasm_batch(
        &self,
        batch_id_payload: Vec<(H256, Bytes)>,
        fn_name: Bytes,
        wasm: Bytes,
        argument: Option<Bytes>,
        at: Option<BlockHash>,
    ) -> RpcResult<Vec<Bytes>>;

    #[method(name = "gear_readMetahash")]
    fn read_metahash(&self, program_id: H256, at: Option<BlockHash>) -> RpcResult<H256>;
}

/// A struct that implements the [`GearApi`](/gclient/struct.GearApi.html).
pub struct Gear<C, P> {
    // If you have more generics, no need to Gear<C, M, N, P, ...>
    // just use a tuple like Gear<C, (M, N, P, ...)>
    client: Arc<C>,
    allowance_multiplier: u64,
    max_batch_size: u64,
    _marker: std::marker::PhantomData<P>,
}

impl<C, P> Gear<C, P> {
    /// Creates a new instance of the Gear Rpc helper.
    pub fn new(client: Arc<C>, allowance_multiplier: u64, max_batch_size: u64) -> Self {
        Self {
            client,
            allowance_multiplier,
            max_batch_size,
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

    fn get_api_version(&self, at_hash: <Block as BlockT>::Hash) -> Result<u32, CallError> {
        self.client
            .runtime_api()
            .api_version::<dyn GearRuntimeApi<Block>>(at_hash)
            .map_err(|e| into_call_err(e, "Failed to get gear runtime api version"))?
            .ok_or_else(|| {
                CallError::Custom(ErrorObject::owned(
                    8000,
                    "Gear runtime api wasn't found in the runtime",
                    None::<String>,
                ))
            })
    }

    fn calculate_gas_info(
        &self,
        at_hash: <Block as BlockT>::Hash,
        source: H256,
        kind: HandleKind,
        payload: Vec<u8>,
        value: u128,
        allow_other_panics: bool,
        min_limit: Option<u64>,
    ) -> RpcResult<GasInfo> {
        let api_version = self.get_api_version(at_hash)?;

        self.run_with_api_copy(|api| {
            if api_version < 2 {
                #[allow(deprecated)]
                api.calculate_gas_info_before_version_2(
                    at_hash,
                    source,
                    kind,
                    payload,
                    value,
                    allow_other_panics,
                    min_limit,
                )
            } else {
                api.calculate_gas_info(
                    at_hash,
                    source,
                    kind,
                    payload,
                    value,
                    allow_other_panics,
                    min_limit,
                    Some(self.allowance_multiplier),
                )
            }
        })
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

fn into_call_err(error: impl ToString, desc: &'static str) -> CallError {
    CallError::Custom(ErrorObject::owned(8000, desc, Some(error.to_string())))
}

#[async_trait]
impl<C, Block> GearApiServer<<Block as BlockT>::Hash, Result<u64, Vec<u8>>> for Gear<C, Block>
where
    Block: BlockT,
    C: 'static + ProvideRuntimeApi<Block> + HeaderBackend<Block>,
    C::Api: GearRuntimeApi<Block>,
{
    fn get_init_create_gas_spent(
        &self,
        source: H256,
        code_id: H256,
        payload: Bytes,
        value: u128,
        allow_other_panics: bool,
        at: Option<<Block as BlockT>::Hash>,
    ) -> RpcResult<GasInfo> {
        let at_hash = at.unwrap_or_else(|| self.client.info().best_hash);

        let GasInfo { min_limit, .. } = self.calculate_gas_info(
            at_hash,
            source,
            HandleKind::InitByHash(CodeId::from_origin(code_id)),
            payload.to_vec(),
            value,
            allow_other_panics,
            None,
        )?;

        self.calculate_gas_info(
            at_hash,
            source,
            HandleKind::InitByHash(CodeId::from_origin(code_id)),
            payload.to_vec(),
            value,
            allow_other_panics,
            Some(min_limit),
        )
    }

    fn get_init_upload_gas_spent(
        &self,
        source: H256,
        code: Bytes,
        payload: Bytes,
        value: u128,
        allow_other_panics: bool,
        at: Option<<Block as BlockT>::Hash>,
    ) -> RpcResult<GasInfo> {
        let at_hash = at.unwrap_or_else(|| self.client.info().best_hash);

        let GasInfo { min_limit, .. } = self.calculate_gas_info(
            at_hash,
            source,
            HandleKind::Init(code.to_vec()),
            payload.to_vec(),
            value,
            allow_other_panics,
            None,
        )?;

        self.calculate_gas_info(
            at_hash,
            source,
            HandleKind::Init(code.to_vec()),
            payload.to_vec(),
            value,
            allow_other_panics,
            Some(min_limit),
        )
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
        let at_hash = at.unwrap_or_else(|| self.client.info().best_hash);

        let GasInfo { min_limit, .. } = self.calculate_gas_info(
            at_hash,
            source,
            HandleKind::Handle(ProgramId::from_origin(dest)),
            payload.to_vec(),
            value,
            allow_other_panics,
            None,
        )?;

        self.calculate_gas_info(
            at_hash,
            source,
            HandleKind::Handle(ProgramId::from_origin(dest)),
            payload.to_vec(),
            value,
            allow_other_panics,
            Some(min_limit),
        )
    }

    fn get_reply_gas_spent(
        &self,
        source: H256,
        message_id: H256,
        payload: Bytes,
        value: u128,
        allow_other_panics: bool,
        at: Option<<Block as BlockT>::Hash>,
    ) -> RpcResult<GasInfo> {
        let at_hash = at.unwrap_or_else(|| self.client.info().best_hash);

        let GasInfo { min_limit, .. } = self.calculate_gas_info(
            at_hash,
            source,
            HandleKind::Reply(
                MessageId::from_origin(message_id),
                ReplyCode::Success(SuccessReplyReason::Manual),
            ),
            payload.to_vec(),
            value,
            allow_other_panics,
            None,
        )?;

        self.calculate_gas_info(
            at_hash,
            source,
            HandleKind::Reply(
                MessageId::from_origin(message_id),
                ReplyCode::Success(SuccessReplyReason::Manual),
            ),
            payload.to_vec(),
            value,
            allow_other_panics,
            Some(min_limit),
        )
    }

    fn read_state(
        &self,
        program_id: H256,
        payload: Bytes,
        at: Option<<Block as BlockT>::Hash>,
    ) -> RpcResult<Bytes> {
        let at_hash = at.unwrap_or_else(|| self.client.info().best_hash);

        let api_version = self.get_api_version(at_hash)?;

        if api_version < 2 {
            self.run_with_api_copy(|api| {
                #[allow(deprecated)]
                api.read_state_before_version_2(at_hash, program_id, payload.to_vec())
            })
            .map(Bytes)
        } else {
            self.run_with_api_copy(|api| {
                api.read_state(
                    at_hash,
                    program_id,
                    payload.to_vec(),
                    Some(self.allowance_multiplier),
                )
            })
            .map(Bytes)
        }
    }

    fn read_state_batch(
        &self,
        batch_id_payload: Vec<(H256, Bytes)>,
        at: Option<<Block as BlockT>::Hash>,
    ) -> RpcResult<Vec<Bytes>> {
        if batch_id_payload.len() > self.max_batch_size as usize {
            return Err(CallError::Custom(ErrorObject::owned(
                8000,
                "Runtime error",
                Some(format!(
                    "Batch size must be lower than {:?}",
                    self.max_batch_size
                )),
            ))
            .into());
        }

        let at_hash = at.unwrap_or_else(|| self.client.info().best_hash);

        let api_version = self.get_api_version(at_hash)?;

        if api_version < 2 {
            batch_id_payload
                .into_iter()
                .map(|(program_id, payload)| {
                    self.run_with_api_copy(|api| {
                        #[allow(deprecated)]
                        api.read_state_before_version_2(at_hash, program_id, payload.0)
                    })
                    .map(Bytes)
                })
                .collect()
        } else {
            batch_id_payload
                .into_iter()
                .map(|(program_id, payload)| {
                    self.run_with_api_copy(|api| {
                        api.read_state(
                            at_hash,
                            program_id,
                            payload.0,
                            Some(self.allowance_multiplier),
                        )
                    })
                    .map(Bytes)
                })
                .collect()
        }
    }

    fn read_state_using_wasm(
        &self,
        program_id: H256,
        payload: Bytes,
        fn_name: Bytes,
        wasm: Bytes,
        argument: Option<Bytes>,
        at: Option<<Block as BlockT>::Hash>,
    ) -> RpcResult<Bytes> {
        let at_hash = at.unwrap_or_else(|| self.client.info().best_hash);

        let api_version = self.get_api_version(at_hash)?;

        if api_version < 2 {
            self.run_with_api_copy(|api| {
                #[allow(deprecated)]
                api.read_state_using_wasm_before_version_2(
                    at_hash,
                    program_id,
                    payload.to_vec(),
                    fn_name.to_vec(),
                    wasm.to_vec(),
                    argument.map(|v| v.to_vec()),
                )
                .map(|r| r.map(Bytes))
            })
        } else {
            self.run_with_api_copy(|api| {
                api.read_state_using_wasm(
                    at_hash,
                    program_id,
                    payload.to_vec(),
                    fn_name.to_vec(),
                    wasm.to_vec(),
                    argument.map(|v| v.to_vec()),
                    Some(self.allowance_multiplier),
                )
                .map(|r| r.map(Bytes))
            })
        }
    }

    fn read_state_using_wasm_batch(
        &self,
        batch_id_payload: Vec<(H256, Bytes)>,
        fn_name: Bytes,
        wasm: Bytes,
        argument: Option<Bytes>,
        at: Option<<Block as BlockT>::Hash>,
    ) -> RpcResult<Vec<Bytes>> {
        if batch_id_payload.len() > self.max_batch_size as usize {
            return Err(CallError::Custom(ErrorObject::owned(
                8000,
                "Runtime error",
                Some(format!(
                    "Batch size must be lower than {:?}",
                    self.max_batch_size
                )),
            ))
            .into());
        }

        let at_hash = at.unwrap_or_else(|| self.client.info().best_hash);

        let api_version = self.get_api_version(at_hash)?;

        if api_version < 2 {
            batch_id_payload
                .into_iter()
                .map(|(program_id, payload)| {
                    self.run_with_api_copy(|api| {
                        #[allow(deprecated)]
                        api.read_state_using_wasm_before_version_2(
                            at_hash,
                            program_id,
                            payload.to_vec(),
                            fn_name.clone().to_vec(),
                            wasm.clone().to_vec(),
                            argument.clone().map(|v| v.to_vec()),
                        )
                        .map(|r| r.map(Bytes))
                    })
                })
                .collect()
        } else {
            batch_id_payload
                .into_iter()
                .map(|(program_id, payload)| {
                    self.run_with_api_copy(|api| {
                        api.read_state_using_wasm(
                            at_hash,
                            program_id,
                            payload.to_vec(),
                            fn_name.clone().to_vec(),
                            wasm.clone().to_vec(),
                            argument.clone().map(|v| v.to_vec()),
                            Some(self.allowance_multiplier),
                        )
                        .map(|r| r.map(Bytes))
                    })
                })
                .collect()
        }
    }

    fn read_metahash(
        &self,
        program_id: H256,
        at: Option<<Block as BlockT>::Hash>,
    ) -> RpcResult<H256> {
        let at_hash = at.unwrap_or_else(|| self.client.info().best_hash);

        let api_version = self.get_api_version(at_hash)?;

        if api_version < 2 {
            #[allow(deprecated)]
            self.run_with_api_copy(|api| api.read_metahash_before_version_2(at_hash, program_id))
        } else {
            self.run_with_api_copy(|api| {
                api.read_metahash(at_hash, program_id, Some(self.allowance_multiplier))
            })
        }
    }
}
