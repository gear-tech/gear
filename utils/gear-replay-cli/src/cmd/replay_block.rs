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

//! Replaying a block on top of the corresponding chain state

use crate::{
    build_executor, fetch_block, fetch_header, full_extensions,
    shared_parameters::SharedParams,
    state::{LiveState, SnapState, State},
    state_machine_call, BlockHashOrNumber, LOG_TARGET,
};
use clap::Parser;
use parity_scale_codec::{Decode, Encode};
#[cfg(feature = "always-wasm")]
use sc_executor::sp_wasm_interface::ExtendedHostFunctions;
#[cfg(all(not(feature = "always-wasm"), feature = "vara-native"))]
use service::VaraExecutorDispatch;
use sp_runtime::{
    traits::{Block as BlockT, Header as HeaderT, One},
    DeserializeOwned,
};
use std::fmt::Debug;
use substrate_rpc_client::ws_client;

/// Replay block subcommand
#[derive(Clone, Debug, Parser)]
pub struct ReplayBlockCmd<Block: BlockT> {
    /// The ws uri from which to fetch the block.
    ///
    /// If `state` is `Live`, this can be ignored since the `uri` of the `LiveState` is used for
    /// both fetching state and block.
    #[arg(
		long,
		value_parser = crate::parse::url
	)]
    pub block_ws_uri: Option<String>,

    /// The state type to use.
    #[command(subcommand)]
    pub state: State<Block>,

    /// Forces `Gear::run()` inherent to be placed in the block.
    ///
    /// In case the `Gear::run()` extrinsic has been dropped from a block due to panic, this flag
    /// can be used to force the `Gear::run()` inherent to be placed in the block to reproduce the
    /// issue.
    #[arg(long, short)]
    force_run: bool,
}

impl<Block: BlockT> ReplayBlockCmd<Block> {
    fn block_ws_uri_and_hash(&self) -> (String, Option<BlockHashOrNumber<Block>>) {
        match (&self.block_ws_uri, &self.state) {
            (Some(block_ws_uri), State::Snap(SnapState { block, .. })) => {
                (block_ws_uri.to_owned(), block.clone())
            }
            (Some(_block_ws_uri), State::Live(LiveState { uri, block, .. })) => {
                log::warn!(target: LOG_TARGET, "--block-uri is ignored when fetching live state");
                (uri.clone(), block.clone())
            }
            (None, State::Live(LiveState { uri, block, .. })) => (uri.clone(), block.clone()),
            (None, State::Snap { .. }) => {
                panic!("either `--block-uri` must be provided, or state must be `live`");
            }
        }
    }
}

pub(crate) async fn run<Block>(
    shared: SharedParams,
    command: ReplayBlockCmd<Block>,
) -> sc_cli::Result<()>
where
    Block: BlockT + DeserializeOwned,
    Block::Header: DeserializeOwned,
{
    let (block_ws_uri, execute_at) = command.block_ws_uri_and_hash();

    // Initialize the RPC client.
    let rpc = ws_client(&block_ws_uri).await?;

    let current_hash = match execute_at {
        Some(b) => Some(b.as_hash(&rpc).await?),
        _ => None,
    };

    log::info!(target: LOG_TARGET, "Fetching block {current_hash:?} ");
    let block = fetch_block::<Block>(&rpc, current_hash).await?;

    let ext = match command.state {
        State::Live(live_state) => {
            let prev_block_live_state = live_state.prev_block_live_state().await?;
            State::Live(prev_block_live_state).to_ext(None).await?
        }

        State::Snap(snap_state) => {
            let ext = State::Snap(snap_state).to_ext(None).await?;

            let header_previous = fetch_header::<Block>(&rpc, Some(ext.header.hash())).await?;
            let expected = *block.header().number() - One::one();
            if *header_previous.number() != expected {
                let message = format!(
                    "Expected snapshot for block number {expected}, but got a snapshot for {}",
                    header_previous.number()
                );
                return Err(message.into());
            }

            ext
        }
    };

    let (mut header, mut extrinsics) = block.deconstruct();

    // A digest item gets added when the runtime is processing the block, so we need to pop
    // the last one to be consistent with what a gossiped block would contain.
    header.digest_mut().pop();

    // Timestamp inherent is always present hence `extrinsics` vector is not empty
    assert!(!extrinsics.is_empty());

    // Create executor, suitable for usage in conjunction with the preferred execution strategy.
    #[cfg(all(not(feature = "always-wasm"), feature = "vara-native"))]
    let executor = build_executor::<VaraExecutorDispatch>(&shared);
    #[cfg(feature = "always-wasm")]
    let executor = build_executor::<
        ExtendedHostFunctions<
            sp_io::SubstrateHostFunctions,
            (
                gear_runtime_interface::gear_ri::HostFunctions,
                gear_runtime_interface::sandbox::HostFunctions,
                sp_crypto_ec_utils::bls12_381::host_calls::HostFunctions,
            ),
        >,
    >(&shared);

    // If asked, re-insert the `Gear::run()` extrinsic into the block (in case it's been dropped).
    if command.force_run {
        let (changes, gear_run_encoded) = state_machine_call::<Block, _>(
            &ext,
            &executor,
            "GearApi_gear_run_extrinsic",
            &None::<u64>.encode(),
            full_extensions(),
        )?;
        assert!(changes.is_empty());

        // Encoded `Gear::run` extrinsic with `max_gas` being `None`: vec![16_u8, 4, 104, 6, 0].
        // While the length (first byte) and the `max_gas` (the last set of bytes) can vary, the
        // middle part including the pallet number (104) and the call index (6) is always the same.

        // `Gear::run()`, if present, is always the last in the block.
        let maybe_gear_run_idx = extrinsics.len() - 1;
        let actual_encoded = extrinsics[maybe_gear_run_idx].encode();

        // See if the last extrinsic in the block contains the constant part of the `Gear::run()`
        if !actual_encoded
            .windows(3)
            .any(|window| window == &gear_run_encoded[1..4])
        {
            let gear_run_tx = <Block as BlockT>::Extrinsic::decode(&mut &gear_run_encoded[..])
                .expect("Failed to decode `Gear::run()` extrinsic");
            extrinsics.push(gear_run_tx);
        }
    }

    let block = Block::new(header, extrinsics);

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
        state_machine_call::<Block, _>(&ext, &executor, method, &payload, full_extensions())?;
    log::info!(
        target: LOG_TARGET,
        "Core_execute_block for block {current_hash:?} completed"
    );

    Ok(())
}
