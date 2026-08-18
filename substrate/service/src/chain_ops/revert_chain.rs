// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use crate::error::Error;
use log::info;
use sc_client_api::{Backend, UsageProvider};
use sp_runtime::traits::{Block as BlockT, NumberFor, Zero};
use std::sync::Arc;

/// Performs a revert of `blocks` blocks.
pub fn revert_chain<B, BA, C>(
    client: Arc<C>,
    backend: Arc<BA>,
    blocks: NumberFor<B>,
) -> Result<(), Error>
where
    B: BlockT,
    C: UsageProvider<B>,
    BA: Backend<B>,
{
    let reverted = backend.revert(blocks, false)?;
    let info = client.usage_info().chain;

    if reverted.0.is_zero() {
        info!("There aren't any non-finalized blocks to revert.");
    } else {
        info!(
            "Reverted {} blocks. Best: #{} ({})",
            reverted.0, info.best_number, info.best_hash
        );

        if reverted.0 > blocks {
            info!(
                "Number of reverted blocks is higher than requested \
				because of reverted leaves higher than the best block."
            )
        }
    }
    Ok(())
}
