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

use clap::Parser;
use codec::Encode;
use runtime_primitives::Block;
#[cfg(feature = "always-wasm")]
use sc_executor::sp_wasm_interface::ExtendedHostFunctions;
#[cfg(all(not(feature = "always-wasm"), feature = "gear-native"))]
use service::GearExecutorDispatch;
#[cfg(all(not(feature = "always-wasm"), feature = "vara-native"))]
use service::VaraExecutorDispatch;
use sp_runtime::{
    generic::SignedBlock,
    traits::{Block as BlockT, Header as HeaderT, One},
};
use sp_state_machine::ExecutionStrategy;
use std::fmt::Debug;
use substrate_rpc_client::{ws_client, ChainApi};
use util::*;

pub(crate) const LOG_TARGET: &str = "remote-ext::cli";

mod parse;
mod util;

const VARA_SS58_PREFIX: u8 = 137;
const GEAR_SS58_PREFIX: u8 = 42;

pub(crate) type HashFor<B> = <B as BlockT>::Hash;
pub(crate) type NumberFor<B> = <<B as BlockT>::Header as HeaderT>::Number;

#[derive(Clone, Debug)]
pub(crate) enum BlockHashOrNumber<B: BlockT> {
    Hash(HashFor<B>),
    Number(NumberFor<B>),
}

#[derive(Clone, Debug, Parser)]
struct Opt {
    /// The RPC url.
    #[arg(
		short,
		long,
		value_parser = parse::url,
		default_value = "wss://archive-rpc.vara-network.io:443"
	)]
    uri: String,

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
}

#[tokio::main]
async fn main() -> sc_cli::Result<()> {
    let options = Opt::parse();

    sp_tracing::try_init_simple();

    let ss58_prefix = match options.uri.contains("vara") {
        true => VARA_SS58_PREFIX,
        false => GEAR_SS58_PREFIX,
    };
    sp_core::crypto::set_default_ss58_version(ss58_prefix.try_into().unwrap());

    // Initialize the RPC client.
    // get the block number associated with this block.
    let block_ws_uri = options.uri.clone();
    let rpc = ws_client(&block_ws_uri).await?;

    let block_hash_or_num = match options.block {
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

    // Get the state at the height corresponging to previous block.
    let previous_hash =
        block_number_to_hash::<Block>(&rpc, current_number.saturating_sub(One::one())).await?;
    log::info!(
        target: LOG_TARGET,
        "Fetching state from {:?} at {:?}",
        options.uri,
        previous_hash,
    );
    let ext = build_externalities::<Block>(
        options.uri,
        Some(previous_hash),
        options.pallet,
        options.child_tree,
    )
    .await?;

    log::info!(target: LOG_TARGET, "Fetching block {:?} ", current_hash);
    let block = fetch_block::<Block>(&rpc, current_hash).await?;

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

    // for now, hardcoded for the sake of simplicity. We might customize them one day.
    #[cfg(not(feature = "try-runtime"))]
    let payload = block.encode();
    #[cfg(feature = "try-runtime")]
    let payload = (block, false, false, "none").encode();
    #[cfg(not(feature = "try-runtime"))]
    let method = "Core_execute_block";
    #[cfg(feature = "try-runtime")]
    let method = "TryRuntime_execute_block";

    #[cfg(not(feature = "always-wasm"))]
    let strategy = ExecutionStrategy::NativeElseWasm;
    #[cfg(feature = "always-wasm")]
    let strategy = ExecutionStrategy::AlwaysWasm;

    let (_changes, _enc_res) = state_machine_call(
        &ext,
        &executor,
        method,
        &payload,
        full_extensions(),
        strategy,
    )?;
    log::info!(
        target: LOG_TARGET,
        "Core_execute_block for block {} completed",
        current_number
    );

    Ok(())
}
