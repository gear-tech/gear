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
use codec::{Decode, Encode, Joiner};
#[cfg(feature = "always-wasm")]
use sc_executor::sp_wasm_interface::ExtendedHostFunctions;
#[cfg(all(not(feature = "always-wasm"), feature = "vara-native"))]
use service::VaraExecutorDispatch;
use sp_runtime::{
    generic::SignedBlock,
    traits::{Block as BlockT, Header as HeaderT, One},
    ApplyExtrinsicResult, DeserializeOwned, Saturating,
};
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
    let ext =
        build_externalities::<Block>(shared.uri.clone(), Some(previous_hash), vec![], true).await?;

    log::info!(target: LOG_TARGET, "Fetching block {:?} ", current_hash);
    let block = fetch_block::<Block>(&rpc, current_hash).await?;

    // A digest item gets added when the runtime is processing the block, so we need to pop
    // the last one to be consistent with what a gossiped block would contain.
    let (header, extrinsics) = block.deconstruct();

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

    let (_changes, _enc_res) = state_machine_call(
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

    // Encoded `Gear::run` extrinsic: length byte 12 (3 << 2) + 104th pallet + 6th extrinsic
    let gear_run_tx = vec![12_u8, 4, 104, 6];

    // Drop the timestamp extrinsic which is always the first in the block
    let extrinsics = extrinsics.into_iter().skip(1).collect::<Vec<_>>();

    for extrinsic in extrinsics {
        let tx_encoded = extrinsic.encode();
        if tx_encoded != gear_run_tx {
            // Apply all extrinsics in the block except for the timestamp and gear::run
            let _ = state_machine_call(
                &ext,
                &executor,
                "BlockBuilder_apply_extrinsic",
                &tx_encoded,
                full_extensions(),
            )?;
        }
    }

    // Applying the `gear_run()` in the end
    let (_changes, enc_res) = state_machine_call(
        &ext,
        &executor,
        "BlockBuilder_apply_extrinsic",
        &gear_run_tx,
        full_extensions(),
    )?;
    let r = ApplyExtrinsicResult::decode(&mut &enc_res[..]).unwrap();
    log::info!(
        target: LOG_TARGET,
        "BlockBuilder_apply_extrinsic done with result {:?}",
        r
    );

    Ok(())
}
