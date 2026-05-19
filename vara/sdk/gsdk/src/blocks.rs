// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! This module provides useful functions for working with block stream.

use crate::{Api, Error, GearConfig, Result};
use futures::prelude::*;
use std::pin::pin;

/// Block retrieved from a node.
pub type Block = subxt::blocks::Block<GearConfig, subxt::OnlineClient<GearConfig>>;

/// Events from some block.
pub type Events = subxt::events::Events<GearConfig>;

/// Checks whether the blocks are progressing.
pub async fn are_progressing<E>(blocks: impl Stream<Item = Result<Block, E>>) -> Result<bool>
where
    Error: From<E>,
{
    let mut blocks = pin!(blocks);

    let current_block = blocks
        .next()
        .await
        .transpose()?
        .ok_or(Error::SubscriptionDied)?;
    let next_block = blocks
        .next()
        .await
        .transpose()?
        .ok_or(Error::SubscriptionDied)?;

    Ok(current_block.number() != next_block.number())
}

impl Api {
    /// Checks whether the blocks on the node is progressing.
    pub async fn is_progressing(&self) -> Result<bool> {
        let blocks = self.blocks().subscribe_all().await?;

        are_progressing(blocks).await
    }
}
