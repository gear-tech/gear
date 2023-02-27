// This file is part of Gear.

// Copyright (C) 2022 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use super::EventProcessor;
use crate::{Error, Result};
use async_trait::async_trait;
use gsdk::{
    config::GearConfig,
    ext::sp_core::H256,
    metadata::{gear::Event as GearEvent, Event},
    types::Blocks,
};
use subxt::events::Events;

/// Event listener that allows catching and processing events propagated through
/// the network.
///
/// # Examples
///
/// ```
/// use gclient::GearApi;
/// # use gclient::Result;
///
/// #[tokio::test]
/// async fn listener_test() -> Result<()> {
///     let api = GearApi::dev().await?;
///     let mut listener = api.subscribe().await?;
///     assert!(listener.blocks_running().await?);
///     Ok(())
/// }
/// ```
pub struct EventListener(pub(crate) Blocks);

#[async_trait(?Send)]
impl EventProcessor for EventListener {
    fn not_waited() -> Error {
        unreachable!()
    }

    async fn proc<T>(&mut self, predicate: impl Fn(Event) -> Option<T> + Copy) -> Result<T> {
        while let Some(events) = self.0.next_events().await {
            if let Some(res) = self.proc_events_inner(events?, predicate) {
                return Ok(res);
            }
        }

        Err(Self::not_waited())
    }

    async fn proc_many<T>(
        &mut self,
        predicate: impl Fn(Event) -> Option<T>,
        validate: impl Fn(Vec<T>) -> (Vec<T>, bool),
    ) -> Result<Vec<T>> {
        let mut res = vec![];

        while let Some(events) = self.0.next_events().await {
            for event in events?.iter() {
                if let Some(data) = predicate(event?.as_root_event::<Event>()?) {
                    res.push(data);
                }
            }

            let finished: bool;
            (res, finished) = validate(res);

            if finished {
                break;
            }
        }

        Ok(res)
    }
}

impl EventListener {
    /// Look through finalized blocks to find the
    /// [`QueueProcessingReverted`](https://docs.gear.rs/pallet_gear/pallet/enum.Event.html#variant.QueueProcessingReverted)
    /// event.
    pub async fn queue_processing_reverted(&mut self) -> Result<H256> {
        while let Some(events) = self.0.next_events().await {
            let events = events?;
            let events_bh = events.block_hash();

            if let Some(res) = self.proc_events_inner(events, |e| {
                matches!(e, Event::Gear(GearEvent::QueueProcessingReverted)).then_some(events_bh)
            }) {
                return Ok(res);
            }
        }

        Err(Self::not_waited())
    }

    /// Reads the next event from the stream and returns the repsective block
    /// hash.
    pub async fn next_block_hash(&mut self) -> Result<H256> {
        Ok(self
            .0
            .next_events()
            .await
            .ok_or(Error::EventNotFound)??
            .block_hash())
    }

    /// Check whether at least one new block has been produced after the
    /// `previous` block.
    pub async fn blocks_running_since(&mut self, previous: H256) -> Result<bool> {
        let current = self.next_block_hash().await?;

        Ok(current != previous)
    }

    /// Check whether new blocks are produced as expected.
    pub async fn blocks_running(&mut self) -> Result<bool> {
        let previous = self.next_block_hash().await?;

        self.blocks_running_since(previous).await
    }

    fn proc_events_inner<T>(
        &mut self,
        events: Events<GearConfig>,
        predicate: impl Fn(Event) -> Option<T>,
    ) -> Option<T> {
        events
            .iter()
            .filter_map(|event| predicate(event.ok()?.as_root_event::<Event>().ok()?))
            .next()
    }
}
