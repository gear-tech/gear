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
    HashOf, MaybeHashOf, StateHashWithQueueSize,
    db::{AnnounceStorageRO, BlockMetaStorageRO, CodesStorageRO, HashStorageRO},
};
use ethexe_runtime_common::state::{
    Dispatch, DispatchStash, Expiring, Mailbox, MailboxMessage, MemoryPages, MemoryPagesInner,
    MemoryPagesRegionInner, MessageQueue, PayloadLookup, Program, ProgramState, UserMailbox,
    Waitlist,
};
use gprimitives::{CodeId, H256};
use parity_scale_codec::Decode;
use std::{
    any::TypeId,
    collections::{BTreeMap, BTreeSet, VecDeque},
};

/// Collects all content-addressed blobs reachable from program states.
struct BlobCollector<'a, S: ?Sized> {
    storage: &'a S,
    /// Dedup of blobs pushed into [`Self::blobs`], keyed by CAS hash alone.
    collected: BTreeSet<H256>,
    /// Dedup of graph traversal, keyed by `(TypeId, H256)`.
    ///
    /// Two unrelated types may serialize to the same bytes and therefore
    /// share a CAS hash — notably any empty `BTreeMap`/`VecDeque`, which
    /// SCALE-encodes as `[0x00]` and so makes empty `Waitlist`, `Mailbox`,
    /// `UserMailbox`, `DispatchStash` and `MessageQueue` indistinguishable
    /// at the storage level. Deduping traversal by hash alone would cause
    /// the second visit to skip its own children and drop their reachable
    /// blobs from the dump.
    visited: BTreeSet<(TypeId, H256)>,
    blobs: Vec<Vec<u8>>,
}

impl<S: HashStorageRO + ?Sized> BlobCollector<'_, S> {
    /// Read raw bytes from CAS by hash and record them as a blob.
    ///
    /// Use for leaf blobs that have no children to traverse (original code,
    /// page data, stored payload). No-op if the hash is zero or the blob was
    /// already collected.
    fn read_and_collect(&mut self, hash: H256) -> Result<()> {
        if hash.is_zero() || !self.collected.insert(hash) {
            return Ok(());
        }

        let data = self
            .storage
            .read_by_hash(hash)
            .with_context(|| format!("missing CAS blob for hash {hash}"))?;

        self.blobs.push(data);
        Ok(())
    }

    /// Read, record and decode a blob whose children must be traversed.
    ///
    /// Traversal is deduplicated per `(TypeId, H256)` so that the same bytes
    /// reached under two different types each get their children walked;
    /// blob storage is still deduplicated per `H256`.
    fn read_and_decode<T: Decode + 'static>(&mut self, hash: H256) -> Result<Option<T>> {
        if hash.is_zero() {
            return Ok(None);
        }

        if !self.visited.insert((TypeId::of::<T>(), hash)) {
            return Ok(None);
        }

        let data = self
            .storage
            .read_by_hash(hash)
            .with_context(|| format!("missing CAS blob for hash {hash}"))?;

        if self.collected.insert(hash) {
            self.blobs.push(data.clone());
        }

        let value = T::decode(&mut &data[..])
            .with_context(|| format!("failed to decode blob at hash {hash}"))?;
        Ok(Some(value))
    }

    fn collect_maybe_hash<T: Decode + 'static>(
        &mut self,
        maybe: MaybeHashOf<T>,
    ) -> Result<Option<T>> {
        match maybe.to_inner() {
            Some(hash) => self.read_and_decode(hash.inner()),
            None => Ok(None),
        }
    }

    fn collect_payload(&mut self, payload: &PayloadLookup) -> Result<()> {
        if let PayloadLookup::Stored(hash) = payload {
            self.read_and_collect(hash.inner())?;
        }
        Ok(())
    }

    fn collect_message_queue(&mut self, maybe: MaybeHashOf<MessageQueue>) -> Result<()> {
        let Some(queue) = self.collect_maybe_hash(maybe)? else {
            return Ok(());
        };

        for dispatch in VecDeque::from(queue) {
            // All fields except payload data are already included
            // in the message queue blob (see collect_maybe_hash above)
            let Dispatch {
                payload,
                id: _,
                kind: _,
                source: _,
                value: _,
                details: _,
                context: _,
                message_type: _,
                call: _,
            } = dispatch;

            self.collect_payload(&payload)?;
        }

        Ok(())
    }

    fn collect_waitlist(&mut self, maybe: MaybeHashOf<Waitlist>) -> Result<()> {
        let Some(waitlist) = self.collect_maybe_hash(maybe)? else {
            return Ok(());
        };

        // `_message_id`, `expiry` and all fields of `Dispatch` except payload data are already included
        // in the waitlist blob (see collect_maybe_hash above)
        for (
            _message_id,
            Expiring {
                value:
                    Dispatch {
                        payload,
                        id: _,
                        kind: _,
                        source: _,
                        value: _,
                        details: _,
                        context: _,
                        message_type: _,
                        call: _,
                    },
                expiry: _,
            },
        ) in BTreeMap::from(waitlist)
        {
            self.collect_payload(&payload)?;
        }

        Ok(())
    }

    fn collect_dispatch_stash(&mut self, maybe: MaybeHashOf<DispatchStash>) -> Result<()> {
        let Some(stash) = self.collect_maybe_hash(maybe)? else {
            return Ok(());
        };

        // `_message_id`, `_maybe_actor`, `expiry` are already included
        // in the stash blob (see collect_maybe_hash above)
        for (
            _message_id,
            Expiring {
                value: (dispatch, _maybe_actor),
                expiry: _,
            },
        ) in BTreeMap::from(stash)
        {
            // All fields except payload data are already included
            // in the dispatch stash blob (see collect_maybe_hash above)
            let Dispatch {
                payload,
                id: _,
                kind: _,
                source: _,
                value: _,
                details: _,
                context: _,
                message_type: _,
                call: _,
            } = dispatch;

            self.collect_payload(&payload)?;
        }

        Ok(())
    }

    fn collect_user_mailbox(&mut self, hash: HashOf<UserMailbox>) -> Result<()> {
        let Some(user_mailbox) = self.read_and_decode::<UserMailbox>(hash.inner())? else {
            return Ok(());
        };

        // `_message_id` is already included in the user mailbox blob (see read_and_decode above)
        for (_message_id, expiring) in BTreeMap::from(user_mailbox) {
            // All fields except payload data are already included
            // in the user mailbox blob (see read_and_decode above)
            let Expiring {
                value:
                    MailboxMessage {
                        payload,
                        value: _,
                        message_type: _,
                    },
                expiry: _,
            } = expiring;
            self.collect_payload(&payload)?;
        }

        Ok(())
    }

    fn collect_mailbox(&mut self, maybe: MaybeHashOf<Mailbox>) -> Result<()> {
        let Some(mailbox) = self.collect_maybe_hash(maybe)? else {
            return Ok(());
        };

        // `_actor_id` is already included in the mailbox blob (see collect_maybe_hash above)
        for (_actor_id, user_mailbox_hash) in BTreeMap::from(mailbox) {
            self.collect_user_mailbox(user_mailbox_hash)?;
        }

        Ok(())
    }

    fn collect_memory_pages(&mut self, maybe: MaybeHashOf<MemoryPages>) -> Result<()> {
        let Some(pages) = self.collect_maybe_hash(maybe)? else {
            return Ok(());
        };

        for region_hash in MemoryPagesInner::from(pages) {
            let Some(region) = self.collect_maybe_hash(region_hash)? else {
                continue;
            };

            // `_page` is already included in the region blob (see collect_maybe_hash above)
            for (_page, page_data_hash) in MemoryPagesRegionInner::from(region) {
                self.read_and_collect(page_data_hash.inner())?;
            }
        }

        Ok(())
    }

    fn collect_program_state(&mut self, state_hash: H256) -> Result<()> {
        let Some(ProgramState {
            program,
            canonical_queue,
            injected_queue,
            waitlist_hash,
            stash_hash,
            mailbox_hash,
            // balance and executable_balance are already included
            // in the program state blob (see read_and_decode below)
            balance: _,
            executable_balance: _,
        }) = self.read_and_decode::<ProgramState>(state_hash)?
        else {
            return Ok(());
        };

        // Collect allocations and memory pages.
        if let Program::Active(active) = &program {
            let _ = self.collect_maybe_hash(active.allocations_hash)?;
            self.collect_memory_pages(active.pages_hash)?;
        }

        // Collect message queues.
        self.collect_message_queue(canonical_queue.hash)?;
        self.collect_message_queue(injected_queue.hash)?;

        // Collect waitlist.
        self.collect_waitlist(waitlist_hash)?;

        // Collect dispatch stash.
        self.collect_dispatch_stash(stash_hash)?;

        // Collect mailbox.
        self.collect_mailbox(mailbox_hash)?;

        Ok(())
    }
}

impl StateDump {
    /// Collect a state dump from the database for a given block hash.
    pub fn collect_from_storage(
        storage: &(impl AnnounceStorageRO + CodesStorageRO + BlockMetaStorageRO + HashStorageRO),
        block_hash: H256,
    ) -> Result<Self> {
        let block_meta = storage.block_meta(block_hash);

        let announce_hash = block_meta
            .last_committed_announce
            .context("no committed announce found for block")?;

        let codes_queue = block_meta
            .codes_queue
            .with_context(|| format!("codes queue not found for block {block_hash}"))?;

        if !codes_queue.is_empty() {
            // StorageDump does not include codes queue, so after re-genesis the queue will be lost.
            log::warn!(
                "Codes queue is not empty at block {block_hash:?}. This may cause hanging codes after re-genesis."
            );
        }

        let mut collector = BlobCollector {
            storage,
            collected: BTreeSet::new(),
            visited: BTreeSet::new(),
            blobs: Vec::new(),
        };

        // Collect all valid codes into blobs.
        let codes = storage.valid_codes();
        for code_id in &codes {
            let code_hash = CodeId::into_bytes(*code_id).into();
            collector.read_and_collect(code_hash)?;
        }

        let program_states = storage
            .announce_program_states(announce_hash)
            .with_context(|| format!("program states not found for announce {announce_hash}"))?;

        // Collect programs and their state trees.
        let mut programs = BTreeMap::new();

        // `canonical_queue_size` and `injected_queue_size` are not included in the program state blob
        for (
            program_id,
            StateHashWithQueueSize {
                hash: state_hash,
                canonical_queue_size: _,
                injected_queue_size: _,
            },
        ) in &program_states
        {
            let code_id = storage
                .program_code_id(*program_id)
                .with_context(|| format!("code id not found for program {program_id}"))?;

            programs.insert(*program_id, (code_id, *state_hash));

            collector.collect_program_state(*state_hash)?;
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
