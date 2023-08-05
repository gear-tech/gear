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

//! Replaying a block against the live chain state

use crate::{parse, util::*, BlockHashOrNumber, SharedParams, LOG_TARGET};
use clap::Parser;
use codec::{Decode, Joiner};
#[cfg(feature = "always-wasm")]
use sc_executor::sp_wasm_interface::ExtendedHostFunctions;
#[cfg(all(not(feature = "always-wasm"), feature = "gear-native"))]
use service::GearExecutorDispatch;
#[cfg(all(not(feature = "always-wasm"), feature = "vara-native"))]
use service::VaraExecutorDispatch;
use sp_runtime::{
    generic::SignedBlock,
    traits::{Block as BlockT, Header as HeaderT, One},
    ApplyExtrinsicResult, DeserializeOwned, Saturating,
};
use sp_state_machine::ExecutionStrategy;
use std::{fmt::Debug, str::FromStr};
use substrate_rpc_client::{ws_client, ChainApi};

/// GearRun subcommand
#[derive(Clone, Debug, Parser)]
pub struct GearRunCmd<Block: BlockT> {
    /// The block hash or number as of which the state (including the runtime) is fetched.
    /// If omitted, the latest finalized block is used.
    #[arg(
		short,
		long,
		value_parser = parse::block,
	)]
    block: Option<BlockHashOrNumber<Block>>,
}

pub(crate) async fn gear_run<Block>(
    shared: SharedParams,
    command: GearRunCmd<Block>,
) -> sc_cli::Result<()>
where
    Block: BlockT + DeserializeOwned,
    Block::Header: DeserializeOwned,
    Block::Hash: FromStr,
    <Block::Hash as FromStr>::Err: Debug,
{
    // Initialize the RPC client.
    // get the block number associated with this block.
    let block_ws_uri = shared.uri.clone();
    let rpc = ws_client(&block_ws_uri).await?;

    let block_hash_or_num = match command.block.clone() {
        Some(b) => b,
        None => {
            let height = ChainApi::<
                (),
                <Block as BlockT>::Hash,
                <Block as BlockT>::Header,
                SignedBlock<Block>,
            >::finalized_head(&rpc)
            .await
            .map_err(rpc_err_handler)?;

            log::info!(
                target: LOG_TARGET,
                "Block is not provided, setting it to the latest finalized head: {:?}",
                height
            );

            BlockHashOrNumber::Hash(height)
        }
    };

    let (block_number, block_hash) = match block_hash_or_num {
        BlockHashOrNumber::Number(n) => (n, block_number_to_hash::<Block>(&rpc, n).await?),
        BlockHashOrNumber::Hash(hash) => (block_hash_to_number::<Block>(&rpc, hash).await?, hash),
    };

    // Get the state at the height corresponging to the requested block.
    log::info!(
        target: LOG_TARGET,
        "Fetching state from {:?} at {:?}",
        shared.uri,
        block_hash,
    );
    let ext =
        build_externalities::<Block>(shared.uri.clone(), Some(block_hash), vec![], true).await?;

    let next_hash =
        block_number_to_hash::<Block>(&rpc, block_number.saturating_add(One::one())).await?;
    log::info!(target: LOG_TARGET, "Fetching block {:?} ", next_hash);
    let block = fetch_block::<Block>(&rpc, next_hash).await?;

    // A digest item gets added when the runtime is processing the block, so we need to pop
    // the last one to be consistent with what a gossiped block would contain.
    let (mut header, extrinsics) = block.deconstruct();
    header.digest_mut().pop();
    let block = Block::new(header, extrinsics);

    // Create executor, suitable for usage in conjunction with the preferred execution strategy.
    #[cfg(all(not(feature = "always-wasm"), feature = "vara-native"))]
    let executor = build_executor::<VaraExecutorDispatch>();
    #[cfg(all(not(feature = "always-wasm"), feature = "gear-native"))]
    let executor = build_executor::<GearExecutorDispatch>();
    #[cfg(feature = "always-wasm")]
    let executor = build_executor::<
        ExtendedHostFunctions<
            sp_io::SubstrateHostFunctions,
            (
                gear_runtime_interface::gear_ri::HostFunctions,
                gear_runtime_interface::sandbox::HostFunctions,
            ),
        >,
    >();

    #[cfg(not(feature = "always-wasm"))]
    let strategy = ExecutionStrategy::NativeElseWasm;
    #[cfg(feature = "always-wasm")]
    let strategy = ExecutionStrategy::AlwaysWasm;

    let (_changes, _enc_res) = state_machine_call(
        &ext,
        &executor,
        "Core_initialize_block",
        &vec![].and(block.header()),
        full_extensions(),
        strategy,
    )?;
    log::info!(target: LOG_TARGET, "Core_initialize_block completed");

    // Encoded Gear::run() extrinsic
    let tx = vec![4_u8, 104, 6];

    let (_changes, enc_res) = state_machine_call(
        &ext,
        &executor,
        "BlockBuilder_apply_extrinsic",
        &vec![].and(&tx),
        full_extensions(),
        strategy,
    )?;
    let r = ApplyExtrinsicResult::decode(&mut &enc_res[..]).unwrap();
    log::info!(
        target: LOG_TARGET,
        "BlockBuilder_apply_extrinsic done with result {:?}",
        r
    );

    Ok(())
}
