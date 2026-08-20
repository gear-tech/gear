// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use crate::error::Error;
use codec::Encode;
use futures::{future, prelude::*};
use log::info;
use sp_runtime::{
    generic::BlockId,
    traits::{Block as BlockT, NumberFor, One, SaturatedConversion, Zero},
};

use sc_client_api::{BlockBackend, HeaderBackend, UsageProvider};
use std::{io::Write, pin::Pin, sync::Arc, task::Poll};

/// Performs the blocks export.
pub fn export_blocks<B, C>(
    client: Arc<C>,
    mut output: impl Write + 'static,
    from: NumberFor<B>,
    to: Option<NumberFor<B>>,
    binary: bool,
) -> Pin<Box<dyn Future<Output = Result<(), Error>>>>
where
    C: HeaderBackend<B> + BlockBackend<B> + UsageProvider<B> + 'static,
    B: BlockT,
{
    let mut block = from;

    let last = match to {
        Some(v) if v.is_zero() => One::one(),
        Some(v) => v,
        None => client.usage_info().chain.best_number,
    };

    let mut wrote_header = false;

    // Exporting blocks is implemented as a future, because we want the operation to be
    // interruptible.
    //
    // Every time we write a block to the output, the `Future` re-schedules itself and returns
    // `Poll::Pending`.
    // This makes it possible either to interleave other operations in-between the block exports,
    // or to stop the operation completely.
    let export = future::poll_fn(move |cx| {
        let client = &client;

        if last < block {
            return Poll::Ready(Err("Invalid block range specified".into()));
        }

        if !wrote_header {
            info!("Exporting blocks from #{} to #{}", block, last);
            if binary {
                let last_: u64 = last.saturated_into::<u64>();
                let block_: u64 = block.saturated_into::<u64>();
                let len: u64 = last_ - block_ + 1;
                output.write_all(&len.encode())?;
            }
            wrote_header = true;
        }

        match client
            .block_hash_from_id(&BlockId::number(block))?
            .map(|hash| client.block(hash))
            .transpose()?
            .flatten()
        {
            Some(block) => {
                if binary {
                    output.write_all(&block.encode())?;
                } else {
                    serde_json::to_writer(&mut output, &block)
                        .map_err(|e| format!("Error writing JSON: {}", e))?;
                }
            }
            None => return Poll::Ready(Ok(())),
        }
        if (block % 10000u32.into()).is_zero() {
            info!("#{}", block);
        }
        if block == last {
            return Poll::Ready(Ok(()));
        }
        block += One::one();

        // Re-schedule the task in order to continue the operation.
        cx.waker().wake_by_ref();
        Poll::Pending
    });

    Box::pin(export)
}
