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

//! Helper functions for creating remote externalities, executor etc.

use crate::{Opt, LOG_TARGET};

use frame_remote_externalities::{
    Builder, Mode, OnlineConfig, RemoteExternalities, TestExternalities,
};
use sc_executor::{NativeElseWasmExecutor, NativeExecutionDispatch, WasmExecutor};
use sp_core::{
    offchain::{
        testing::{TestOffchainExt, TestTransactionPoolExt},
        OffchainDbExt, OffchainWorkerExt, TransactionPoolExt,
    },
    storage::well_known_keys,
    testing::TaskExecutor,
    traits::{CallContext, TaskExecutorExt},
    twox_128,
};
use sp_externalities::Extensions;
use sp_keystore::{testing::KeyStore, KeystoreExt};
use sp_rpc::{list::ListOrValue, number::NumberOrHex};
use sp_runtime::{
    traits::{Block as BlockT, Header as HeaderT},
    DeserializeOwned,
};
use sp_state_machine::{
    backend::BackendRuntimeCode, ExecutionStrategy, OverlayedChanges, StateMachine,
};
use std::{fmt::Debug, str::FromStr, sync::Arc};
use substrate_rpc_client::{ChainApi, WsClient};

pub(crate) fn build_executor<D: NativeExecutionDispatch>() -> NativeElseWasmExecutor<D> {
    let heap_pages = Some(2048);
    let max_runtime_instances = 8;
    let runtime_cache_size = 2;

    NativeElseWasmExecutor::<D>::new_with_wasm_executor(WasmExecutor::new(
        sc_executor::WasmExecutionMethod::Interpreted,
        heap_pages,
        max_runtime_instances,
        None,
        runtime_cache_size,
    ))
}

pub(crate) fn hash_of<Block: BlockT>(hash_str: &str) -> sc_cli::Result<Block::Hash>
where
    Block::Hash: FromStr,
    <Block::Hash as FromStr>::Err: Debug,
{
    hash_str
        .parse::<<Block as BlockT>::Hash>()
        .map_err(|e| format!("Could not parse block hash: {:?}", e).into())
}

pub(crate) async fn build_externalities<Block: BlockT + DeserializeOwned>(
    options: Opt,
) -> sc_cli::Result<RemoteExternalities<Block>>
where
    Block::Hash: FromStr,
    Block::Header: DeserializeOwned,
    Block::Hash: DeserializeOwned,
    <Block::Hash as FromStr>::Err: Debug,
{
    let at = match options.at {
        Some(at_str) => Some(hash_of::<Block>(&at_str)?),
        None => None,
    };
    let builder = Builder::<Block>::new().mode(Mode::Online(OnlineConfig {
        at,
        transport: options.uri.to_owned().into(),
        state_snapshot: None,
        pallets: options.pallet.clone(),
        child_trie: options.child_tree,
        hashed_keys: vec![
            // we always download the code
            well_known_keys::CODE.to_vec(),
            // we will always download this key, since it helps detect if we should do
            // runtime migration or not.
            [twox_128(b"System"), twox_128(b"LastRuntimeUpgrade")].concat(),
            [twox_128(b"System"), twox_128(b"Number")].concat(),
        ],
        hashed_prefixes: vec![],
    }));

    // build the main ext.
    Ok(builder.build().await?)
}

pub(crate) async fn next_hash_of<Block: BlockT>(
    rpc: &WsClient,
    hash: Block::Hash,
) -> sc_cli::Result<Block::Hash>
where
    Block: BlockT + DeserializeOwned,
    Block::Header: DeserializeOwned,
{
    let number = ChainApi::<(), Block::Hash, Block::Header, ()>::header(rpc, Some(hash))
        .await
        .map_err(rpc_err_handler)
        .and_then(|maybe_header| maybe_header.ok_or("header_not_found").map(|h| *h.number()))?;

    let next = number + sp_runtime::traits::One::one();

    let next_hash = match ChainApi::<(), Block::Hash, Block::Header, ()>::block_hash(
        rpc,
        Some(ListOrValue::Value(NumberOrHex::Number(
            next.try_into()
                .map_err(|_| "failed to convert number to block number")?,
        ))),
    )
    .await
    .map_err(rpc_err_handler)?
    {
        ListOrValue::Value(t) => t.expect("value passed in; value comes out; qed"),
        _ => unreachable!(),
    };

    Ok(next_hash)
}

pub(crate) fn rpc_err_handler(error: impl Debug) -> &'static str {
    log::error!(target: LOG_TARGET, "rpc error: {:?}", error);
    "rpc error."
}

/// Execute the given `method` and `data` on top of `ext`, returning the results (encoded) and the
/// state `changes`.
pub(crate) fn state_machine_call<D: NativeExecutionDispatch + 'static>(
    ext: &TestExternalities,
    executor: &NativeElseWasmExecutor<D>,
    method: &'static str,
    data: &[u8],
    extensions: Extensions,
) -> sc_cli::Result<(OverlayedChanges, Vec<u8>)> {
    let mut changes = Default::default();
    let encoded_results = StateMachine::new(
        &ext.backend,
        &mut changes,
        executor,
        method,
        data,
        extensions,
        &BackendRuntimeCode::new(&ext.backend).runtime_code()?,
        TaskExecutor::new(),
        CallContext::Offchain,
    )
    .execute(ExecutionStrategy::NativeElseWasm)
    .map_err(|e| format!("failed to execute '{}': {}", method, e))
    .map_err::<sc_cli::Error, _>(Into::into)?;

    Ok((changes, encoded_results))
}

/// Build all extensions that are typically used
pub(crate) fn full_extensions() -> Extensions {
    let mut extensions = Extensions::default();
    extensions.register(TaskExecutorExt::new(TaskExecutor::new()));
    let (offchain, _offchain_state) = TestOffchainExt::new();
    let (pool, _pool_state) = TestTransactionPoolExt::new();
    extensions.register(OffchainDbExt::new(offchain.clone()));
    extensions.register(OffchainWorkerExt::new(offchain));
    extensions.register(KeystoreExt(Arc::new(KeyStore::new())));
    extensions.register(TransactionPoolExt::new(pool));

    extensions
}
