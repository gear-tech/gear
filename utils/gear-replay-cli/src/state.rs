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

use clap::Subcommand;

use crate::{BlockHashOrNumber, LOG_TARGET, parse, rpc_err_handler};
use frame_remote_externalities::{
    Builder, Mode, OfflineConfig, OnlineConfig, RemoteExternalities, SnapshotConfig,
};
use sp_core::{storage::well_known_keys, twox_128};
use sp_runtime::{
    DeserializeOwned,
    traits::{Block as BlockT, Header as HeaderT},
};
use std::{fmt::Debug, path::PathBuf};
use substrate_rpc_client::{ChainApi, ws_client};

/// The `Snap` variant [`State`]
#[derive(Debug, Clone, clap::Args)]
pub struct SnapState<Block: BlockT> {
    /// Snapshot path.
    #[arg(short = 'p', long = "path", alias = "snapshot-path")]
    path: Option<PathBuf>,

    /// The block hash or number we want to replay.
    ///
    /// This doesn't have any effect on the state but suggests the block number to
    /// fetch and replay.
    #[arg(
		short, long, value_parser = parse::block)]
    pub block: Option<BlockHashOrNumber<Block>>,
}

/// The `Live` variant [`State`]
#[derive(Debug, Clone, clap::Args)]
pub struct LiveState<Block: BlockT> {
    /// The RPC url.
    #[arg(
		short,
		long,
		value_parser = parse::url,
		default_value = "wss://archive-rpc.vara.network:443"
	)]
    pub uri: String,

    /// The block hash or number we want to replay. If omitted, the latest finalized block is used.
    /// The blockchain state at previous block with respect to this parameter will be scraped.
    #[arg(
		short,
		long,
		value_parser = parse::block,
	)]
    pub block: Option<BlockHashOrNumber<Block>>,

    /// Pallet(s) to scrape. Comma-separated multiple items are also accepted.
    /// If empty, entire chain state will be scraped.
    ///
    /// This is equivalent to passing `xx_hash_64(pallet)` to `--hashed_prefixes`.
    #[arg(short, long, num_args = 1..)]
    pallet: Vec<String>,

    /// Storage entry key prefixes to scrape and inject into the test externalities. Pass as 0x
    /// prefixed hex strings. By default, all keys are scraped and included.
    #[arg(long = "prefix", value_parser = parse::hash, num_args = 1..)]
    pub hashed_prefixes: Vec<String>,

    /// Fetch the child-keys as well.
    ///
    /// Default is `false`, if specific `--pallets` are specified, `true` otherwise. In other
    /// words, if you scrape the whole state the child tree data is included out of the box.
    /// Otherwise, it must be enabled explicitly using this flag.
    #[arg(long)]
    child_tree: bool,
}

impl<Block: BlockT> LiveState<Block>
where
    Block: BlockT + DeserializeOwned,
    Block::Header: DeserializeOwned,
{
    /// Consumes a `LiveState` and creates a new `LiveState` for the previous block.
    ///
    /// Useful for opertations like when you want to execute a block, but also need the state of the
    /// block *before* it.
    pub async fn prev_block_live_state(self) -> sc_cli::Result<LiveState<Block>> {
        // We want to execute the block `block`, therefore need the state of the block *before* it.
        let rpc = ws_client(&self.uri).await?;

        let maybe_block_hash = match self.block {
            Some(b) => Some(b.as_hash(&rpc).await?),
            _ => None,
        };

        // Get the block number requested by the user, or the current block number if they
        // didn't specify one.
        let previous_hash =
            ChainApi::<(), Block::Hash, Block::Header, ()>::header(&rpc, maybe_block_hash)
                .await
                .map_err(rpc_err_handler)
                .and_then(|maybe_header| {
                    maybe_header
                        .ok_or("header_not_found")
                        .map(|h| *h.parent_hash())
                })?;

        Ok(LiveState {
            block: Some(BlockHashOrNumber::Hash(previous_hash)),
            ..self
        })
    }
}

/// The source of runtime *state* to use.
#[derive(Debug, Clone, Subcommand)]
pub enum State<Block: BlockT> {
    /// Use a state snapshot as the source of runtime state.
    Snap(SnapState<Block>),

    /// Use a live chain as the source of runtime state.
    Live(LiveState<Block>),
}

impl<Block: BlockT> State<Block>
where
    Block: BlockT + DeserializeOwned,
    Block::Header: DeserializeOwned,
{
    /// Create [`RemoteExternalities`] based on a state.
    pub async fn to_ext(
        &self,
        state_snapshot: Option<SnapshotConfig>,
    ) -> sc_cli::Result<RemoteExternalities<Block>> {
        let builder = match self {
            State::Snap(SnapState { path, .. }) => {
                let path = path
                    .as_ref()
                    .ok_or_else(|| "no snapshot path provided".to_string())?;

                Builder::<Block>::new().mode(Mode::Offline(OfflineConfig {
                    state_snapshot: SnapshotConfig::new(path),
                }))
            }
            State::Live(LiveState {
                pallet,
                uri,
                block,
                child_tree,
                hashed_prefixes,
            }) => {
                // Initialize the RPC client.
                // get the block number associated with this block.
                let rpc = ws_client(uri).await?;

                let at = match block {
                    Some(b) => Some(b.as_hash(&rpc).await?),
                    _ => None,
                };

                log::info!(
                    target: LOG_TARGET,
                    "Preparing to fetch state from {uri:?} at {at:?}",
                );

                let hashed_prefixes = hashed_prefixes
                    .iter()
                    .map(|p_str| {
                        hex::decode(p_str).map_err(|e| {
                            format!(
                                "Error decoding `hashed_prefixes` hex string entry '{p_str:?}' to bytes: {e:?}",
                            )
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                Builder::<Block>::new().mode(Mode::Online(OnlineConfig {
                    at,
                    transport: uri.to_owned().into(),
                    state_snapshot,
                    pallets: pallet.clone(),
                    child_trie: *child_tree,
                    hashed_keys: vec![
                        // we always download the code, but we almost always won't use it, based on
                        // `Runtime`.
                        well_known_keys::CODE.to_vec(),
                        // we will always download this key, since it helps detect if we should do
                        // runtime migration or not.
                        [twox_128(b"System"), twox_128(b"LastRuntimeUpgrade")].concat(),
                        [twox_128(b"System"), twox_128(b"Number")].concat(),
                    ],
                    hashed_prefixes,
                }))
            }
        };

        // build the main ext.
        Ok(builder.build().await?)
    }
}
