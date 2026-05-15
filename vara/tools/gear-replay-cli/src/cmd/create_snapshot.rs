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

//! Create a chain state snapshot

use crate::{
    LOG_TARGET,
    shared_parameters::SharedParams,
    state::{LiveState, State},
};
use clap::Parser;
use sp_runtime::{DeserializeOwned, traits::Block as BlockT};
use std::fmt::Debug;
use substrate_rpc_client::{StateApi, ws_client};

/// Create snapshot subcommand
#[derive(Clone, Debug, Parser)]
pub struct CreateSnapshotCmd<Block: BlockT> {
    /// The source of the snapshot. Must be a remote node.
    #[clap(flatten)]
    pub from: LiveState<Block>,

    /// The snapshot path to write to.
    ///
    /// If not provided `<spec-name>-<spec-version>@<block-hash>.snap` will be used.
    #[clap(index = 1)]
    pub snapshot_path: Option<String>,
}

pub(crate) async fn run<Block>(
    _shared: SharedParams,
    command: CreateSnapshotCmd<Block>,
) -> sc_cli::Result<()>
where
    Block: BlockT + DeserializeOwned,
    Block::Header: DeserializeOwned,
{
    let snapshot_path = command.snapshot_path;

    let path = match snapshot_path {
        Some(path) => path,
        None => {
            let rpc = ws_client(&command.from.uri).await.unwrap();
            let remote_spec = StateApi::<Block::Hash>::runtime_version(&rpc, None)
                .await
                .unwrap();
            let block_hash = match command.from.block {
                Some(ref h) => format!("{h}"),
                _ => "latext".to_owned(),
            };
            let path_str = format!(
                "{}-{}@{}.snap",
                remote_spec.spec_name.to_lowercase(),
                remote_spec.spec_version,
                block_hash,
            );
            log::info!(target: LOG_TARGET, "snapshot path not provided (-s), using '{path_str}'");
            path_str
        }
    };

    let _ = State::Live(command.from).to_ext(Some(path.into())).await?;

    Ok(())
}
