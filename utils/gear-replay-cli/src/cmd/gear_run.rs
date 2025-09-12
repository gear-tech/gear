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

//! Apply the `gear::run()` extrinsic on top of state changes introduced in a block

use crate::{
    BlockHashOrNumber, LOG_TARGET, build_executor, fetch_block, full_extensions,
    shared_parameters::SharedParams,
    state::{LiveState, SnapState, State},
    state_machine_call,
};
use clap::Parser;
use parity_scale_codec::{Decode, Encode, Joiner};
#[cfg(feature = "always-wasm")]
use sc_executor::sp_wasm_interface::ExtendedHostFunctions;
#[cfg(all(not(feature = "always-wasm"), feature = "vara-native"))]
use service::VaraExecutorDispatch;
use sp_runtime::{
    ApplyExtrinsicResult, DeserializeOwned,
    traits::{Block as BlockT, Header as HeaderT},
};
use std::fmt::Debug;
use substrate_rpc_client::ws_client;

/// GearRun subcommand
#[derive(Clone, Debug, Parser)]
pub struct GearRunCmd<Block: BlockT> {
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
}

impl<Block: BlockT> GearRunCmd<Block> {
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
    command: GearRunCmd<Block>,
) -> sc_cli::Result<()>
where
    Block: BlockT + DeserializeOwned,
    Block::Header: DeserializeOwned,
{
    let (block_ws_uri, execute_at) = command.block_ws_uri_and_hash();

    // Initialize the RPC client.
    let rpc = ws_client(&block_ws_uri).await?;

    let ext = match command.state {
        State::Live(live_state) => {
            let prev_block_live_state = live_state.prev_block_live_state().await?;
            State::Live(prev_block_live_state).to_ext(None).await?
        }
        State::Snap(snap_state) => State::Snap(snap_state).to_ext(None).await?,
    };

    let current_hash = match execute_at {
        Some(b) => Some(b.as_hash(&rpc).await?),
        _ => None,
    };

    log::info!(target: LOG_TARGET, "Fetching block {current_hash:?} ");
    let block = fetch_block::<Block>(&rpc, current_hash).await?;

    let (mut header, extrinsics) = block.deconstruct();

    // A digest item gets added when the runtime is processing the block, so we need to pop
    // the last one to be consistent with what a gossiped block would contain.
    header.digest_mut().pop();

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
                gear_runtime_interface::gear_webpki::HostFunctions,
            ),
        >,
    >(&shared);

    let (_changes, _enc_res) = state_machine_call::<Block, _>(
        &ext,
        &executor,
        "Core_initialize_block",
        &vec![].and(&header),
        full_extensions(),
    )?;
    log::info!(
        target: LOG_TARGET,
        "Core_initialize_block completed for block {:?}",
        header.number()
    );

    let (_changes, gear_run_encoded) = state_machine_call::<Block, _>(
        &ext,
        &executor,
        "GearApi_gear_run_extrinsic",
        &None::<u64>.encode(),
        full_extensions(),
    )?;

    // Drop the timestamp extrinsic which is always the first in the block
    let extrinsics = extrinsics.into_iter().skip(1).collect::<Vec<_>>();

    let is_gear_run = |x: &Vec<u8>| x.windows(3).any(|window| window == &gear_run_encoded[1..4]);

    for extrinsic in extrinsics {
        let tx_encoded = extrinsic.encode();
        if !is_gear_run(&tx_encoded) {
            // Apply all extrinsics in the block except for the timestamp and gear::run
            let _ = state_machine_call::<Block, _>(
                &ext,
                &executor,
                "BlockBuilder_apply_extrinsic",
                &tx_encoded,
                full_extensions(),
            )?;
        }
    }

    // Applying the `gear_run()` in the end
    let (_changes, enc_res) = state_machine_call::<Block, _>(
        &ext,
        &executor,
        "BlockBuilder_apply_extrinsic",
        &gear_run_encoded,
        full_extensions(),
    )?;
    let r = ApplyExtrinsicResult::decode(&mut &enc_res[..]).unwrap();
    log::info!(
        target: LOG_TARGET,
        "BlockBuilder_apply_extrinsic done with result {r:?}"
    );

    Ok(())
}
