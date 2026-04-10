// This file is part of Gear.
//
// Copyright (C) 2024-2025 Gear Technologies Inc.
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

//! State dump collection from database.

use super::StateDump;
use anyhow::{Context, Result};
use ethexe_common::{
    HashOf, MaybeHashOf,
    db::{AnnounceStorageRO, BlockMetaStorageRO, CodesStorageRO, HashStorageRO},
};
use ethexe_runtime_common::state::{
    Dispatch, DispatchStash, Mailbox, MailboxMessage, MemoryPages, MemoryPagesRegion, MessageQueue,
    PayloadLookup, ProgramState, UserMailbox, Waitlist,
};
use gprimitives::{CodeId, H256};
use parity_scale_codec::Decode;
use std::collections::{BTreeMap, BTreeSet};

/// Collects all content-addressed blobs reachable from program states.
struct BlobCollector<'a, S: ?Sized> {
    storage: &'a S,
    visited: BTreeSet<H256>,
    blobs: Vec<Vec<u8>>,
}

impl<S: HashStorageRO + ?Sized> BlobCollector<'_, S> {
    /// Read raw bytes from CAS by hash, record them as a blob, and return the bytes.
    /// Returns `None` if hash is zero or was already visited.
    fn read_and_collect(&mut self, hash: H256) -> Result<Option<Vec<u8>>> {
        if hash.is_zero() || !self.visited.insert(hash) {
            return Ok(None);
        }

        let data = self
            .storage
            .read_by_hash(hash)
            .with_context(|| format!("missing CAS blob for hash {hash}"))?;

        self.blobs.push(data.clone());
        Ok(Some(data))
    }

    fn read_and_decode<T: Decode>(&mut self, hash: H256) -> Result<Option<T>> {
        self.read_and_collect(hash)?
            .map(|data| {
                T::decode(&mut &data[..])
                    .with_context(|| format!("failed to decode blob at hash {hash}"))
            })
            .transpose()
    }

    fn collect_maybe_hash<T: Decode>(&mut self, maybe: MaybeHashOf<T>) -> Result<Option<T>> {
        match maybe.to_inner() {
            Some(hash) => self.read_and_decode(hash.inner()),
            None => Ok(None),
        }
    }

    fn collect_payload(&mut self, payload: &PayloadLookup) -> Result<()> {
        if let PayloadLookup::Stored(hash) = payload {
            let _ = self.read_and_collect(hash.inner())?;
        }
        Ok(())
    }

    fn collect_dispatch(&mut self, dispatch: &Dispatch) -> Result<()> {
        self.collect_payload(&dispatch.payload)
    }

    fn collect_message_queue(&mut self, maybe: MaybeHashOf<MessageQueue>) -> Result<()> {
        let Some(queue) = self.collect_maybe_hash(maybe)? else {
            return Ok(());
        };

        let dispatches: std::collections::VecDeque<Dispatch> = queue.into();
        for dispatch in &dispatches {
            self.collect_dispatch(dispatch)?;
        }

        Ok(())
    }

    fn collect_waitlist(&mut self, maybe: MaybeHashOf<Waitlist>) -> Result<()> {
        let Some(waitlist) = self.collect_maybe_hash(maybe)? else {
            return Ok(());
        };

        let inner: BTreeMap<_, _> = waitlist.into();
        for expiring in inner.values() {
            self.collect_dispatch(&expiring.value)?;
        }

        Ok(())
    }

    fn collect_dispatch_stash(&mut self, maybe: MaybeHashOf<DispatchStash>) -> Result<()> {
        let Some(stash) = self.collect_maybe_hash(maybe)? else {
            return Ok(());
        };

        let inner: BTreeMap<_, _> = stash.into();
        for expiring in inner.values() {
            self.collect_dispatch(&expiring.value.0)?;
        }

        Ok(())
    }

    fn collect_mailbox_message(&mut self, message: &MailboxMessage) -> Result<()> {
        self.collect_payload(&message.payload)
    }

    fn collect_user_mailbox(&mut self, hash: HashOf<UserMailbox>) -> Result<()> {
        let Some(user_mailbox) = self.read_and_decode::<UserMailbox>(hash.inner())? else {
            return Ok(());
        };

        let inner: BTreeMap<_, _> = user_mailbox.into();
        for expiring in inner.values() {
            self.collect_mailbox_message(&expiring.value)?;
        }

        Ok(())
    }

    fn collect_mailbox(&mut self, maybe: MaybeHashOf<Mailbox>) -> Result<()> {
        let Some(mailbox) = self.collect_maybe_hash(maybe)? else {
            return Ok(());
        };

        let inner: BTreeMap<_, _> = mailbox.into();
        for user_mailbox_hash in inner.values() {
            self.collect_user_mailbox(*user_mailbox_hash)?;
        }

        Ok(())
    }

    fn collect_memory_pages(&mut self, maybe: MaybeHashOf<MemoryPages>) -> Result<()> {
        let Some(pages) = self.collect_maybe_hash(maybe)? else {
            return Ok(());
        };

        let regions: [MaybeHashOf<MemoryPagesRegion>; MemoryPages::REGIONS_AMOUNT] = pages.into();
        for region_hash in regions {
            let Some(region) = self.collect_maybe_hash(region_hash)? else {
                continue;
            };

            let inner: BTreeMap<_, _> = region.into();
            for page_hash in inner.values() {
                let _ = self.read_and_collect(page_hash.inner())?;
            }
        }

        Ok(())
    }

    fn collect_program_state(&mut self, state_hash: H256) -> Result<()> {
        let Some(state) = self.read_and_decode::<ProgramState>(state_hash)? else {
            return Ok(());
        };

        // Collect allocations and memory pages.
        if let ethexe_runtime_common::state::Program::Active(active) = &state.program {
            let _ = self.collect_maybe_hash(active.allocations_hash)?;
            self.collect_memory_pages(active.pages_hash)?;
        }

        // Collect message queues.
        self.collect_message_queue(state.canonical_queue.hash)?;
        self.collect_message_queue(state.injected_queue.hash)?;

        // Collect waitlist.
        self.collect_waitlist(state.waitlist_hash)?;

        // Collect dispatch stash.
        self.collect_dispatch_stash(state.stash_hash)?;

        // Collect mailbox.
        self.collect_mailbox(state.mailbox_hash)?;

        Ok(())
    }
}

impl StateDump {
    /// Collect a state dump from the database for a given block hash.
    pub fn collect_from_storage(
        storage: &(impl AnnounceStorageRO + CodesStorageRO + BlockMetaStorageRO + HashStorageRO),
        block_hash: H256,
    ) -> Result<Self> {
        let announce_hash = storage
            .block_meta(block_hash)
            .last_committed_announce
            .context("no committed announce found for block")?;

        if !storage
            .block_meta(block_hash)
            .codes_queue
            .with_context(|| format!("codes queue not found for block {block_hash}"))?
            .is_empty()
        {
            // StorageDump does not include codes queue, so after re-genesis the queue will be lost.
            log::warn!(
                "Codes queue is not empty at block {block_hash:?}. This may cause hanging codes after re-genesis."
            );
        }

        let mut collector = BlobCollector {
            storage,
            visited: BTreeSet::new(),
            blobs: Vec::new(),
        };

        // Collect all valid codes into blobs.
        let codes = storage.valid_codes();
        for code_id in &codes {
            let code_hash = CodeId::into_bytes(*code_id).into();
            let _ = collector.read_and_collect(code_hash)?;
        }

        let program_states = storage
            .announce_program_states(announce_hash)
            .with_context(|| format!("program states not found for announce {announce_hash}"))?;

        // Collect programs and their state trees.
        let mut programs = BTreeMap::new();
        for (program_id, state_with_queue) in &program_states {
            let code_id = storage
                .program_code_id(*program_id)
                .with_context(|| format!("code id not found for program {program_id}"))?;

            programs.insert(*program_id, (code_id, state_with_queue.hash));

            collector.collect_program_state(state_with_queue.hash)?;
        }

        Ok(StateDump {
            announce_hash,
            block_hash,
            codes,
            programs,
            blobs: collector.blobs,
        })
    }
}
