// This file is part of Gear.
//
// Copyright (C) 2021-2023 Gear Technologies Inc.
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

//! Shared types
use crate::{config::GearConfig, metadata::Event};
use futures::{Stream, StreamExt};
use std::{marker::Unpin, pin::Pin, result::Result as StdResult, task::Poll};
use subxt::{blocks::Block, events::Events as SubxtEvents, Error, OnlineClient};

type SubxtBlock = Block<GearConfig, OnlineClient<GearConfig>>;
type BlockSubscription = Pin<Box<dyn Stream<Item = StdResult<SubxtBlock, Error>> + Send>>;

/// Subscription of finalized blocks.
pub struct Blocks(BlockSubscription);

impl Unpin for Blocks {}

impl Stream for Blocks {
    type Item = StdResult<SubxtBlock, Error>;

    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        let res = futures::ready!(self.0.poll_next_unpin(cx));

        Poll::Ready(res)
    }
}

impl Blocks {
    /// Wait for the next block from the subscription.
    pub async fn next_events(&mut self) -> Option<StdResult<SubxtEvents<GearConfig>, Error>> {
        if let Some(block) = StreamExt::next(self).await {
            Some(block.ok()?.events().await)
        } else {
            None
        }
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
    pub async fn next(&mut self) -> Option<StdResult<Vec<Event>, Error>> {
        self.0.next_events().await.map(|r| {
            r.and_then(|es| {
                es.iter()
                    .map(|ev| ev.and_then(|e| e.as_root_event::<Event>()))
                    .collect::<StdResult<Vec<_>, Error>>()
            })
        })
    }
}

impl From<BlockSubscription> for Events {
    fn from(sub: BlockSubscription) -> Self {
        Self(sub.into())
    }
}
