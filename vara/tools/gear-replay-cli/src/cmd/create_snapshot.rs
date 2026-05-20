// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

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
