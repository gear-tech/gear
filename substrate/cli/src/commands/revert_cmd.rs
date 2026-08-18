// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use crate::{
    error,
    params::{DatabaseParams, GenericNumber, PruningParams, SharedParams},
    CliConfiguration,
};
use clap::Parser;
use sc_client_api::{Backend, UsageProvider};
use sc_service::chain_ops::revert_chain;
use sp_runtime::traits::{Block as BlockT, Header as HeaderT, NumberFor};
use std::{fmt::Debug, str::FromStr, sync::Arc};

/// The `revert` command used revert the chain to a previous state.
#[derive(Debug, Parser)]
pub struct RevertCmd {
    /// Number of blocks to revert.
    #[arg(default_value = "256")]
    pub num: GenericNumber,

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

/// Revert handler for auxiliary data (e.g. consensus).
type AuxRevertHandler<C, BA, B> =
    Box<dyn FnOnce(Arc<C>, Arc<BA>, NumberFor<B>) -> error::Result<()>>;

impl RevertCmd {
    /// Run the revert command
    pub async fn run<B, BA, C>(
        &self,
        client: Arc<C>,
        backend: Arc<BA>,
        aux_revert: Option<AuxRevertHandler<C, BA, B>>,
    ) -> error::Result<()>
    where
        B: BlockT,
        BA: Backend<B>,
        C: UsageProvider<B>,
        <<<B as BlockT>::Header as HeaderT>::Number as FromStr>::Err: Debug,
    {
        let blocks = self.num.parse()?;
        if let Some(aux_revert) = aux_revert {
            aux_revert(client.clone(), backend.clone(), blocks)?;
        }
        revert_chain(client, backend, blocks)?;

        Ok(())
    }
}

impl CliConfiguration for RevertCmd {
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
