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

use crate::{config::GearConfig, metadata::Event, result::Result};
use futures::{Stream, StreamExt};
use serde::Deserialize;
use sp_core::H256;
use std::{marker::Unpin, ops::Deref, pin::Pin, task::Poll};
use subxt::{
    OnlineClient,
    backend::{StreamOfResults, rpc::RpcSubscription},
    blocks::Block,
    events::Events as SubxtEvents,
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

/// Program state change item.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct ProgramStateChange {
    /// Hash of the block that triggered the notification.
    pub block_hash: H256,
    /// List of programs whose states changed in that block.
    pub program_ids: Vec<H256>,
}

/// Subscription of program state changes.
pub struct ProgramStateChanges(RpcSubscription<ProgramStateChange>);

impl ProgramStateChanges {
    pub(crate) fn new(inner: RpcSubscription<ProgramStateChange>) -> Self {
        Self(inner)
    }

    /// Obtain the underlying subscription identifier if available.
    pub fn subscription_id(&self) -> Option<&str> {
        self.0.subscription_id()
    }
}

impl Stream for ProgramStateChanges {
    type Item = Result<ProgramStateChange>;

    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        match futures::ready!(self.0.poll_next_unpin(cx)) {
            Some(Ok(change)) => Poll::Ready(Some(Ok(change))),
            Some(Err(err)) => Poll::Ready(Some(Err(err.into()))),
            None => Poll::Ready(None),
        }
    }
}

impl Unpin for ProgramStateChanges {}

impl Deref for BlockEvents {
    type Target = SubxtEvents<GearConfig>;

    fn deref(&self) -> &Self::Target {
        &self.events
    }
}
