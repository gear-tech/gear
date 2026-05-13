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

use runtime_primitives::Block;
use sc_cli::execution_method_from_cli;
#[cfg(feature = "always-wasm")]
use sc_executor::sp_wasm_interface::HostFunctions;
use sc_executor::{DEFAULT_HEAP_ALLOC_STRATEGY, HeapAllocStrategy, WasmExecutor};
#[cfg(not(feature = "always-wasm"))]
use sc_executor::{NativeElseWasmExecutor, NativeExecutionDispatch};
use sp_core::{
    offchain::{
        OffchainDbExt, OffchainWorkerExt, TransactionPoolExt,
        testing::{TestOffchainExt, TestTransactionPoolExt},
    },
    traits::{CallContext, CodeExecutor},
};
use sp_externalities::Extensions;
use sp_keystore::{KeystoreExt, KeystorePtr, testing::MemoryKeystore};
use sp_rpc::{list::ListOrValue, number::NumberOrHex};
use sp_runtime::{
    DeserializeOwned,
    generic::SignedBlock,
    traits::{Block as BlockT, HashingFor, Header as HeaderT},
};
use sp_state_machine::{
    OverlayedChanges, StateMachine, TestExternalities, backend::BackendRuntimeCode,
};
use std::{
    fmt::{self, Debug},
    sync::Arc,
};
use substrate_rpc_client::{ChainApi, WsClient};

use crate::shared_parameters::SharedParams;

pub const LOG_TARGET: &str = "gear_replay";

pub mod cmd;
mod parse;
mod shared_parameters;
mod state;

pub type HashFor<B> = <B as BlockT>::Hash;
pub type NumberFor<B> = <<B as BlockT>::Header as HeaderT>::Number;

#[derive(Clone, Debug)]
pub enum BlockHashOrNumber<B: BlockT> {
    Hash(HashFor<B>),
    Number(NumberFor<B>),
}

impl<B: BlockT> fmt::Display for BlockHashOrNumber<B> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BlockHashOrNumber::Hash(hash) => {
                write!(f, "{}", hex::encode(hash.as_ref()))
            }
            BlockHashOrNumber::Number(number) => {
                write!(f, "{number}")
            }
        }
    }
}

impl<Block: BlockT> BlockHashOrNumber<Block> {
    pub(crate) async fn as_hash(&self, rpc: &WsClient) -> sc_cli::Result<Block::Hash>
    where
        Block: DeserializeOwned,
        Block::Header: DeserializeOwned,
    {
        match self {
            BlockHashOrNumber::Hash(h) => Ok(*h),
            BlockHashOrNumber::Number(n) => Ok(
                match ChainApi::<(), Block::Hash, Block::Header, ()>::block_hash(
                    rpc,
                    Some(ListOrValue::Value(NumberOrHex::Number(
                        (*n).try_into()
                            .map_err(|_| "failed to convert number to block number")?,
                    ))),
                )
                .await
                .map_err(rpc_err_handler)?
                {
                    ListOrValue::Value(t) => t.expect("value passed in; value comes out; qed"),
                    _ => unreachable!(),
                },
            ),
        }
    }
}

#[cfg(not(feature = "always-wasm"))]
pub(crate) fn build_executor<D: NativeExecutionDispatch>(
    shared: &SharedParams,
) -> NativeElseWasmExecutor<D> {
    let heap_pages =
        shared
            .heap_pages
            .map_or(DEFAULT_HEAP_ALLOC_STRATEGY, |p| HeapAllocStrategy::Static {
                extra_pages: p as _,
            });

    let wasm_executor = WasmExecutor::builder()
        .with_execution_method(execution_method_from_cli(
            shared.wasm_method,
            shared.wasmtime_instantiation_strategy,
        ))
        .with_onchain_heap_alloc_strategy(heap_pages)
        .with_offchain_heap_alloc_strategy(heap_pages)
        .with_allow_missing_host_functions(true)
        .build();

    NativeElseWasmExecutor::<D>::new_with_wasm_executor(wasm_executor)
}

#[cfg(feature = "always-wasm")]
pub(crate) fn build_executor<H: HostFunctions>(shared: &SharedParams) -> WasmExecutor<H> {
    let heap_pages =
        shared
            .heap_pages
            .map_or(DEFAULT_HEAP_ALLOC_STRATEGY, |p| HeapAllocStrategy::Static {
                extra_pages: p as _,
            });

    WasmExecutor::builder()
        .with_execution_method(execution_method_from_cli(
            shared.wasm_method,
            shared.wasmtime_instantiation_strategy,
        ))
        .with_onchain_heap_alloc_strategy(heap_pages)
        .with_offchain_heap_alloc_strategy(heap_pages)
        .with_allow_missing_host_functions(true)
        .build()
}

pub(crate) async fn fetch_block<Block>(
    rpc: &WsClient,
    hash: Option<HashFor<Block>>,
) -> sc_cli::Result<Block>
where
    Block: BlockT + DeserializeOwned,
    Block::Header: DeserializeOwned,
{
    Ok(
        ChainApi::<(), Block::Hash, Block::Header, SignedBlock<Block>>::block(rpc, hash)
            .await
            .map_err(rpc_err_handler)?
            .expect("header exists, block should also exist; qed")
            .block,
    )
}

pub(crate) async fn fetch_header<Block>(
    rpc: &WsClient,
    hash: Option<HashFor<Block>>,
) -> sc_cli::Result<Block::Header>
where
    Block: BlockT + DeserializeOwned,
    Block::Header: DeserializeOwned,
{
    Ok(
        ChainApi::<(), Block::Hash, Block::Header, SignedBlock<Block>>::header(rpc, hash)
            .await
            .map_err(rpc_err_handler)?
            .expect("header should exist"),
    )
}

pub(crate) fn rpc_err_handler(error: impl Debug) -> &'static str {
    log::error!(target: LOG_TARGET, "rpc error: {error:?}");
    "rpc error."
}

/// Execute the given `method` and `data` on top of `ext` using the `executor` and `strategy`.
/// Returning the results (encoded) and the state `changes`.
#[allow(clippy::result_large_err)]
pub(crate) fn state_machine_call<Block: BlockT, Executor: CodeExecutor>(
    ext: &TestExternalities<HashingFor<Block>>,
    executor: &Executor,
    method: &'static str,
    data: &[u8],
    mut extensions: Extensions,
) -> sc_cli::Result<(OverlayedChanges<HashingFor<Block>>, Vec<u8>)> {
    let mut changes = Default::default();
    let encoded_results = StateMachine::new(
        &ext.backend,
        &mut changes,
        executor,
        method,
        data,
        &mut extensions,
        &BackendRuntimeCode::new(&ext.backend).runtime_code()?,
        CallContext::Offchain,
    )
    .execute()
    .map_err(|e| format!("failed to execute '{method}': {e}"))
    .map_err::<sc_cli::Error, _>(Into::into)?;

    Ok((changes, encoded_results))
}

/// Build all extensions that are typically used
pub(crate) fn full_extensions() -> Extensions {
    let mut extensions = Extensions::default();
    let (offchain, _offchain_state) = TestOffchainExt::new();
    let (pool, _pool_state) = TestTransactionPoolExt::new();
    extensions.register(OffchainDbExt::new(offchain.clone()));
    extensions.register(OffchainWorkerExt::new(offchain));
    extensions.register(KeystoreExt(Arc::new(MemoryKeystore::new()) as KeystorePtr));
    extensions.register(TransactionPoolExt::new(pool));

    extensions
}
