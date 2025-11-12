// This file is part of Gear.
//
// Copyright (C) 2021-2025 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Subscription implementation.

use crate::{Result, config::GearConfig, metadata::Event};
use futures::{Stream, StreamExt};
use std::{marker::Unpin, ops::Deref, pin::Pin, task::Poll};
use subxt::{
    OnlineClient, backend::StreamOfResults, blocks::Block, events::Events as SubxtEvents,
    utils::H256,
};

type SubxtBlock = Block<GearConfig, OnlineClient<GearConfig>>;
type BlockSubscription = StreamOfResults<SubxtBlock>;

/// Subscription of finalized blocks.
pub struct Blocks(BlockSubscription);

impl Unpin for Blocks {}

impl Stream for Blocks {
    type Item = Result<SubxtBlock>;

    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        let res = futures::ready!(self.0.poll_next_unpin(cx));

        Poll::Ready(res.map(|inner| inner.map_err(Into::into)))
    }
}

impl Blocks {
    /// Wait for the next block from the subscription.
    pub async fn next_events(&mut self) -> Result<Option<BlockEvents>> {
        let Some(next) = StreamExt::next(self).await else {
            return Ok(None);
        };

        Ok(Some(BlockEvents::new(next?).await?))
    }
}

impl From<BlockSubscription> for Blocks {
    fn from(sub: BlockSubscription) -> Self {
        Self(sub)
    }
}

/// Subscription of events.
pub struct Events(Blocks);

impl Events {
    /// Wait for the next events from the subscription.
    pub async fn next(&mut self) -> Result<Vec<Event>> {
        if let Some(es) = self.0.next_events().await? {
            es.events()
        } else {
            Ok(Default::default())
        }
    }
}

impl From<BlockSubscription> for Events {
    fn from(sub: BlockSubscription) -> Self {
        Self(sub.into())
    }
}

/// Subxt events wrapper with block info
#[derive(Clone, Debug)]
pub struct BlockEvents {
    /// Block hash of the provided events
    block_hash: H256,
    /// subxt events
    events: SubxtEvents<GearConfig>,
}

impl BlockEvents {
    /// Wrap subxt events with block info
    pub async fn new(block: Block<GearConfig, OnlineClient<GearConfig>>) -> Result<Self> {
        Ok(Self {
            block_hash: block.hash(),
            events: block.events().await?,
        })
    }

    /// Get the block hash of the holding events
    pub fn block_hash(&self) -> H256 {
        self.block_hash
    }

    /// Get gear events
    pub fn events(&self) -> Result<Vec<Event>> {
        self.events
            .iter()
            .map(|ev| {
                ev.and_then(|e| e.as_root_event::<Event>())
                    .map_err(Into::into)
            })
            .collect::<Result<Vec<_>>>()
    }
}

impl Deref for BlockEvents {
    type Target = SubxtEvents<GearConfig>;

    fn deref(&self) -> &Self::Target {
        &self.events
    }
}
