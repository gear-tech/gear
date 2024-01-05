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

//! Replaying a block against the live chain state

use crate::{parse, util::*, BlockHashOrNumber, SharedParams, LOG_TARGET};
use clap::Parser;
use codec::{Decode, Encode};
#[cfg(feature = "always-wasm")]
use sc_executor::sp_wasm_interface::ExtendedHostFunctions;
#[cfg(all(not(feature = "always-wasm"), feature = "vara-native"))]
use service::VaraExecutorDispatch;
use sp_runtime::{
    generic::SignedBlock,
    traits::{Block as BlockT, Header as HeaderT, One},
    DeserializeOwned, Saturating,
};
use std::{fmt::Debug, str::FromStr};
use substrate_rpc_client::{ws_client, ChainApi};

/// Replay block subcommand
#[derive(Clone, Debug, Parser)]
pub struct ReplayBlockCmd<Block: BlockT> {
    /// The block hash or number we want to replay. If omitted, the latest finalized block is used.
    /// The blockchain state at previous block with respect to this parameter will be scraped.
    #[arg(
		short,
		long,
		value_parser = parse::block,
	)]
    block: Option<BlockHashOrNumber<Block>>,

    /// Pallet(s) to scrape. Comma-separated multiple items are also accepted.
    /// If empty, entire chain state will be scraped.
    #[arg(short, long, num_args = 1..)]
    pallet: Vec<String>,

    /// Fetch the child-keys as well.
    ///
    /// Default is `false`, if specific `--pallets` are specified, `true` otherwise. In other
    /// words, if you scrape the whole state the child tree data is included out of the box.
    /// Otherwise, it must be enabled explicitly using this flag.
    #[arg(long)]
    child_tree: bool,

    /// Forces `Gear::run()` inherent to be placed in the block.
    ///
    /// In case the `Gear::run()` extrinsic has been dropped from a block due to panic, this flag
    /// can be used to force the `Gear::run()` inherent to be placed in the block to reproduce the
    /// issue.
    #[arg(long, short)]
    force_run: bool,
}

pub(crate) async fn replay_block<Block>(
    shared: SharedParams,
    command: ReplayBlockCmd<Block>,
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

    let (current_number, current_hash) = match block_hash_or_num {
        BlockHashOrNumber::Number(n) => (n, block_number_to_hash::<Block>(&rpc, n).await?),
        BlockHashOrNumber::Hash(hash) => (block_hash_to_number::<Block>(&rpc, hash).await?, hash),
    };

    // Get the state at the height corresponding to previous block.
    let previous_hash =
        block_number_to_hash::<Block>(&rpc, current_number.saturating_sub(One::one())).await?;
    log::info!(
        target: LOG_TARGET,
        "Fetching state from {:?} at {:?}",
        shared.uri,
        previous_hash,
    );
    let ext = build_externalities::<Block>(
        shared.uri.clone(),
        Some(previous_hash),
        command.pallet.clone(),
        command.child_tree,
    )
    .await?;

    log::info!(target: LOG_TARGET, "Fetching block {:?} ", current_hash);
    let block = fetch_block::<Block>(&rpc, current_hash).await?;

    let (mut header, mut extrinsics) = block.deconstruct();

    // A digest item gets added when the runtime is processing the block, so we need to pop
    // the last one to be consistent with what a gossiped block would contain.
    header.digest_mut().pop();

    // In case the `Gear::run()` extrinsic has been dropped due to panic, we re-insert it here.
    // Timestamp inherent is always present hence `extrinsics` vector is not empty
    assert!(!extrinsics.is_empty());

    if command.force_run {
        // Encoded `Gear::run` extrinsic: length byte 12 (3 << 2) + 104th pallet + 6th extrinsic
        let gear_run_encoded = vec![12_u8, 4, 104, 6];
        // `Gear::run()`, is present, is always the last in the block.
        let maybe_gear_run_idx = extrinsics.len() - 1;
        if extrinsics[maybe_gear_run_idx].encode() != gear_run_encoded {
            let gear_run_tx = <Block as BlockT>::Extrinsic::decode(&mut &gear_run_encoded[..])
                .expect("Failed to decode `Gear::run()` extrinsic");
            extrinsics.push(gear_run_tx);
        }
    }

    let block = Block::new(header, extrinsics);

    // Create executor, suitable for usage in conjunction with the preferred execution strategy.
    #[cfg(all(not(feature = "always-wasm"), feature = "vara-native"))]
    let executor = build_executor::<VaraExecutorDispatch>();
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

    // for now, hardcoded for the sake of simplicity. We might customize them one day.
    #[cfg(not(feature = "try-runtime"))]
    let payload = block.encode();
    #[cfg(feature = "try-runtime")]
    let payload = (block, false, false, "none").encode();
    #[cfg(not(feature = "try-runtime"))]
    let method = "Core_execute_block";
    #[cfg(feature = "try-runtime")]
    let method = "TryRuntime_execute_block";

    let (_changes, _enc_res) =
        state_machine_call(&ext, &executor, method, &payload, full_extensions())?;
    log::info!(
        target: LOG_TARGET,
        "Core_execute_block for block {} completed",
        current_number
    );

    Ok(())
}
