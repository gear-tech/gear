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
use service::VaraExecutorDispatch;
use sp_runtime::{
    generic::SignedBlock,
    traits::{Block as BlockT, Header as HeaderT},
};
use std::fmt::Debug;
use substrate_rpc_client::{ws_client, ChainApi};
use util::*;
use vara_runtime::{Block, Runtime};

pub(crate) const LOG_TARGET: &str = "remote-ext::cli";

mod parse;
mod util;

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

    /// The block hash to fetch the state at. If omitted, then the latest finalized head is used.
    #[arg(
		short,
		long,
		value_parser = parse::hash,
	)]
    at: Option<String>,

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

    log::info!(
        target: LOG_TARGET,
        "Fetching state from {:?} at {:?}",
        options.uri,
        options.at
    );

    sp_core::crypto::set_default_ss58_version(
        <Runtime as frame_system::Config>::SS58Prefix::get()
            .try_into()
            .unwrap(),
    );

    let executor = build_executor::<VaraExecutorDispatch>();

    let ext = build_externalities::<Block>(options.clone()).await?;

    // get the block number associated with this block.
    let block_ws_uri = options.uri;
    let rpc = ws_client(&block_ws_uri).await?;
    let next_hash = next_hash_of::<Block>(&rpc, ext.block_hash).await?;

    log::info!(target: LOG_TARGET, "fetching next block: {:?} ", next_hash);

    let block = ChainApi::<
        (),
        <Block as BlockT>::Hash,
        <Block as BlockT>::Header,
        SignedBlock<Block>,
    >::block(&rpc, Some(next_hash))
    .await
    .map_err(rpc_err_handler)?
    .expect("header exists, block should also exist; qed")
    .block;

    // A digest item gets added when the runtime is processing the block, so we need to pop
    // the last one to be consistent with what a gossiped block would contain.
    let (mut header, extrinsics) = block.deconstruct();
    header.digest_mut().pop();
    let block = Block::new(header, extrinsics);

    // for now, hardcoded for the sake of simplicity. We might customize them one day.
    let payload = block.encode();

    let _ = state_machine_call::<VaraExecutorDispatch>(
        &ext,
        &executor,
        "Core_execute_block",
        &payload,
        full_extensions(),
    )?;

    log::info!(target: LOG_TARGET, "Done",);

    Ok(())
}
