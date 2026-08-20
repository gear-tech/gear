// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use crate::{
    error,
    params::{ImportParams, SharedParams},
    CliConfiguration,
};
use clap::Parser;
use sc_client_api::HeaderBackend;
use sc_service::chain_ops::import_blocks;
use sp_runtime::traits::Block as BlockT;
use std::{
    fmt::Debug,
    fs,
    io::{self, Read},
    path::PathBuf,
    sync::Arc,
};

/// The `import-blocks` command used to import blocks.
#[derive(Debug, Parser)]
pub struct ImportBlocksCmd {
    /// Input file or stdin if unspecified.
    #[arg()]
    pub input: Option<PathBuf>,

    /// The default number of 64KB pages to ever allocate for Wasm execution.
    /// Don't alter this unless you know what you're doing.
    #[arg(long, value_name = "COUNT")]
    pub default_heap_pages: Option<u32>,

    /// Try importing blocks from binary format rather than JSON.
    #[arg(long)]
    pub binary: bool,

    #[allow(missing_docs)]
    #[clap(flatten)]
    pub shared_params: SharedParams,

    #[allow(missing_docs)]
    #[clap(flatten)]
    pub import_params: ImportParams,
}

impl ImportBlocksCmd {
    /// Run the import-blocks command
    pub async fn run<B, C, IQ>(&self, client: Arc<C>, import_queue: IQ) -> error::Result<()>
    where
        C: HeaderBackend<B> + Send + Sync + 'static,
        B: BlockT + for<'de> serde::Deserialize<'de>,
        IQ: sc_service::ImportQueue<B> + 'static,
    {
        let file: Box<dyn Read + Send> = match &self.input {
            Some(filename) => Box::new(fs::File::open(filename)?),
            None => Box::new(io::stdin()),
        };

        import_blocks(client, import_queue, file, false, self.binary)
            .await
            .map_err(Into::into)
    }
}

impl CliConfiguration for ImportBlocksCmd {
    fn shared_params(&self) -> &SharedParams {
        &self.shared_params
    }

    fn import_params(&self) -> Option<&ImportParams> {
        Some(&self.import_params)
    }
}
