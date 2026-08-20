// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use crate::{
    error,
    params::{DatabaseParams, GenericNumber, PruningParams, SharedParams},
    CliConfiguration,
};
use clap::Parser;
use log::info;
use sc_client_api::{BlockBackend, HeaderBackend, UsageProvider};
use sc_service::{chain_ops::export_blocks, config::DatabaseSource};
use sp_runtime::traits::{Block as BlockT, Header as HeaderT};
use std::{fmt::Debug, fs, io, path::PathBuf, str::FromStr, sync::Arc};

/// The `export-blocks` command used to export blocks.
#[derive(Debug, Clone, Parser)]
pub struct ExportBlocksCmd {
    /// Output file name or stdout if unspecified.
    #[arg()]
    pub output: Option<PathBuf>,

    /// Specify starting block number.
    /// Default is 1.
    #[arg(long, value_name = "BLOCK")]
    pub from: Option<GenericNumber>,

    /// Specify last block number.
    /// Default is best block.
    #[arg(long, value_name = "BLOCK")]
    pub to: Option<GenericNumber>,

    /// Use binary output rather than JSON.
    #[arg(long)]
    pub binary: bool,

    #[allow(missing_docs)]
    #[clap(flatten)]
    pub shared_params: SharedParams,

    #[allow(missing_docs)]
    #[clap(flatten)]
    pub pruning_params: PruningParams,

    #[allow(missing_docs)]
    #[clap(flatten)]
    pub database_params: DatabaseParams,
}

impl ExportBlocksCmd {
    /// Run the export-blocks command
    pub async fn run<B, C>(
        &self,
        client: Arc<C>,
        database_config: DatabaseSource,
    ) -> error::Result<()>
    where
        B: BlockT,
        C: HeaderBackend<B> + BlockBackend<B> + UsageProvider<B> + 'static,
        <<B::Header as HeaderT>::Number as FromStr>::Err: Debug,
    {
        if let Some(path) = database_config.path() {
            info!("DB path: {}", path.display());
        }

        let from = self
            .from
            .as_ref()
            .and_then(|f| f.parse().ok())
            .unwrap_or(1u32);
        let to = self.to.as_ref().and_then(|t| t.parse().ok());

        let binary = self.binary;

        let file: Box<dyn io::Write> = match &self.output {
            Some(filename) => Box::new(fs::File::create(filename)?),
            None => Box::new(io::stdout()),
        };

        export_blocks(client, file, from.into(), to, binary)
            .await
            .map_err(Into::into)
    }
}

impl CliConfiguration for ExportBlocksCmd {
    fn shared_params(&self) -> &SharedParams {
        &self.shared_params
    }

    fn pruning_params(&self) -> Option<&PruningParams> {
        Some(&self.pruning_params)
    }

    fn database_params(&self) -> Option<&DatabaseParams> {
        Some(&self.database_params)
    }
}
