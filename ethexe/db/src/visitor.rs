// This file is part of Gear.
//
// Copyright (C) 2025 Gear Technologies Inc.
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

use crate::Database;
use ethexe_common::{
    BlockHeader, BlockMeta, Digest, ProgramStates, Schedule, StateHashWithQueueSize,
    db::{BlockMetaStorageRead, CodesStorageRead, OnChainStorageRead},
    events::BlockEvent,
    gear::StateTransition,
};
use ethexe_runtime_common::state::{
    ActiveProgram, Allocations, DispatchStash, Expiring, HashOf, Mailbox, MaybeHashOf, MemoryPages,
    MemoryPagesRegion, MessageQueue, PayloadLookup, Program, ProgramState, Storage, UserMailbox,
    Waitlist,
};
use gear_core::memory::PageBuf;
use gprimitives::{CodeId, H256};
use std::collections::{HashSet, VecDeque};

pub trait DatabaseVisitorStorage:
    OnChainStorageRead + BlockMetaStorageRead + CodesStorageRead + Storage
{
}

impl<T: OnChainStorageRead + BlockMetaStorageRead + CodesStorageRead + Storage>
    DatabaseVisitorStorage for T
{
}

pub trait DatabaseVisitor: Sized {
    type DbError: DatabaseVisitorError;

    fn db(&self) -> &dyn DatabaseVisitorStorage;

    fn on_db_error(&mut self, error: Self::DbError);

    fn visit_chain(&mut self, head: H256, bottom: H256) {
        walk_chain(self, head, bottom)
    }

    fn visit_block(&mut self, block: H256) {
        walk_block(self, block)
    }

    fn visit_block_meta(&mut self, _meta: &BlockMeta) {}

    fn visit_block_header(&mut self, _header: BlockHeader) {}

    fn visit_block_events(&mut self, _events: &[BlockEvent]) {}

    fn visit_block_commitment_queue(&mut self, _queue: &VecDeque<H256>) {}

    fn visit_block_codes_queue(&mut self, _queue: &VecDeque<CodeId>) {}

    fn visit_previous_non_empty_block(&mut self, _block: H256) {}

    fn visit_last_committed_batch(&mut self, _batch: Digest) {}

    fn visit_block_program_states(&mut self, program_states: &ProgramStates) {
        walk_block_program_states(self, program_states)
    }

    fn visit_program_state(&mut self, state: &ProgramState) {
        walk_program_state(self, state)
    }

    fn visit_block_schedule(&mut self, _schedule: &Schedule) {}

    fn visit_block_outcome_is_empty(&mut self, block: H256, outcome_is_empty: bool) {
        walk_block_outcome_is_empty(self, block, outcome_is_empty)
    }

    fn visit_block_outcome(&mut self, _outcome: &[StateTransition]) {}

    fn visit_allocations(&mut self, _allocations: &Allocations) {}

    fn visit_memory_pages(&mut self, memory_pages: &MemoryPages) {
        walk_memory_pages(self, memory_pages)
    }

    fn visit_memory_pages_region(&mut self, memory_pages_region: &MemoryPagesRegion) {
        walk_memory_pages_region(self, memory_pages_region)
    }

    fn visit_page_data(&mut self, _page_data: &[u8]) {}

    fn visit_payload_lookup(&mut self, _payload_lookup: &PayloadLookup) {}

    fn visit_message_queue(&mut self, queue: &MessageQueue) {
        walk_message_queue(self, queue)
    }

    fn visit_waitlist(&mut self, waitlist: &Waitlist) {
        walk_waitlist(self, waitlist)
    }

    fn visit_mailbox(&mut self, mailbox: &Mailbox) {
        walk_mailbox(self, mailbox)
    }

    fn visit_user_mailbox(&mut self, user_mailbox: &UserMailbox) {
        walk_user_mailbox(self, user_mailbox)
    }

    fn visit_dispatch_stash(&mut self, stash: &DispatchStash) {
        walk_dispatch_stash(self, stash)
    }
}

pub trait DatabaseVisitorError {
    /* block header */
    fn no_block_header(block: H256) -> Self;

    /* block */
    fn no_block_events(block: H256) -> Self;
    fn no_block_program_states(block: H256) -> Self;
    fn no_block_schedule(block: H256) -> Self;
    fn no_block_outcome(block: H256) -> Self;
    fn no_block_commitment_queue(block: H256) -> Self;
    fn no_block_codes_queue(block: H256) -> Self;
    fn no_previous_non_empty_block(block: H256) -> Self;
    fn no_last_committed_batch(block: H256) -> Self;

    /* memory */
    fn no_memory_pages(hash: HashOf<MemoryPages>) -> Self;
    fn no_memory_pages_region(hash: HashOf<MemoryPagesRegion>) -> Self;
    fn no_page_data(hash: HashOf<PageBuf>) -> Self;

    /* rest */
    fn no_message_queue(hash: HashOf<MessageQueue>) -> Self;
    fn no_waitlist(hash: HashOf<Waitlist>) -> Self;
    fn no_dispatch_stash(hash: HashOf<DispatchStash>) -> Self;
    fn no_mailbox(hash: HashOf<Mailbox>) -> Self;
    fn no_user_mailbox(hash: HashOf<UserMailbox>) -> Self;
    fn no_allocations(hash: HashOf<Allocations>) -> Self;
    fn no_program_state(hash: H256) -> Self;
}

impl DatabaseVisitorError for () {
    fn no_block_header(_block: H256) -> Self {}

    fn no_block_events(_block: H256) -> Self {}

    fn no_block_program_states(_block: H256) -> Self {}

    fn no_block_schedule(_block: H256) -> Self {}

    fn no_block_outcome(_block: H256) -> Self {}

    fn no_block_commitment_queue(_block: H256) -> Self {}

    fn no_block_codes_queue(_block: H256) -> Self {}

    fn no_previous_non_empty_block(_block: H256) -> Self {}

    fn no_last_committed_batch(_block: H256) -> Self {}

    fn no_memory_pages(_hash: HashOf<MemoryPages>) -> Self {}

    fn no_memory_pages_region(_hash: HashOf<MemoryPagesRegion>) -> Self {}

    fn no_page_data(_hash: HashOf<PageBuf>) -> Self {}

    fn no_message_queue(_hash: HashOf<MessageQueue>) -> Self {}

    fn no_waitlist(_hash: HashOf<Waitlist>) -> Self {}

    fn no_dispatch_stash(_hash: HashOf<DispatchStash>) -> Self {}

    fn no_mailbox(_hash: HashOf<Mailbox>) -> Self {}

    fn no_user_mailbox(_hash: HashOf<UserMailbox>) -> Self {}

    fn no_allocations(_hash: HashOf<Allocations>) -> Self {}

    fn no_program_state(_hash: H256) -> Self {}
}

macro_rules! visit_or_error {
    ($visitor:ident, $hash:ident.$element:ident) => {{
        let x = $visitor.db().$element($hash);
        if let Some(x) = x {
            paste::paste! {
                 $visitor. [< visit_ $element >] (x);
            }
        } else {
            paste::item! {
                $visitor.on_db_error(<_>:: [< no_ $element >] ($hash));
            }
        }
        x
    }};
    ($visitor:ident, &$hash:ident.$element:ident) => {{
        let x = $visitor.db().$element($hash);
        if let Some(x) = &x {
            paste::paste! {
                 $visitor. [< visit_ $element >] (x);
            }
        } else {
            paste::item! {
                $visitor.on_db_error(<_>:: [< no_ $element >] ($hash));
            }
        }
        x
    }};
    ($visitor:ident, $element:ident.as_ref()) => {{
        let x = $visitor.db().$element($element);
        if let Some(x) = x.as_ref() {
            paste::paste! {
                 $visitor. [< visit_ $element >] (x);
            }
        } else {
            paste::item! {
                $visitor.on_db_error(<_>:: [< no_ $element >] ($element));
            }
        }
        x
    }};
}

pub fn walk_chain<E>(visitor: &mut impl DatabaseVisitor<DbError = E>, head: H256, bottom: H256)
where
    E: DatabaseVisitorError,
{
    let mut block = head;
    while block != bottom {
        visitor.visit_block(block);

        let header = visitor.db().block_header(block);
        if let Some(header) = header {
            block = header.parent_hash;
        } else {
            visitor.on_db_error(E::no_block_header(block));
            break;
        }
    }
}

pub fn walk_block<E>(visitor: &mut impl DatabaseVisitor<DbError = E>, block: H256)
where
    E: DatabaseVisitorError,
{
    let meta = visitor.db().block_meta(block);
    visitor.visit_block_meta(&meta);

    visit_or_error!(visitor, block.block_header);

    visit_or_error!(visitor, &block.block_events);

    visit_or_error!(visitor, &block.block_commitment_queue);

    visit_or_error!(visitor, &block.block_codes_queue);

    visit_or_error!(visitor, block.previous_non_empty_block);

    visit_or_error!(visitor, block.last_committed_batch);

    visit_or_error!(visitor, &block.block_program_states);

    // TODO: verification might be required for schedule
    visit_or_error!(visitor, &block.block_schedule);

    // TODO: verification required for codes queue
    let outcome_is_empty = visitor.db().block_outcome_is_empty(block);
    if let Some(outcome_is_empty) = outcome_is_empty {
        visitor.visit_block_outcome_is_empty(block, outcome_is_empty);
    } else {
        visitor.on_db_error(E::no_block_outcome(block));
    }
}

pub fn walk_block_program_states<E>(
    visitor: &mut impl DatabaseVisitor<DbError = E>,
    program_states: &ProgramStates,
) where
    E: DatabaseVisitorError,
{
    for StateHashWithQueueSize {
        hash,
        cached_queue_size: _,
    } in program_states.values().copied()
    {
        visit_or_error!(visitor, &hash.program_state);
    }
}

pub fn walk_program_state<E>(visitor: &mut impl DatabaseVisitor<DbError = E>, state: &ProgramState)
where
    E: DatabaseVisitorError,
{
    let ProgramState {
        program,
        queue,
        waitlist_hash,
        stash_hash,
        mailbox_hash,
        balance: _,
        executable_balance: _,
    } = state;

    if let Program::Active(ActiveProgram {
        allocations_hash,
        pages_hash,
        memory_infix: _,
        initialized: _,
    }) = program
    {
        if let Some(allocations) = allocations_hash.to_inner() {
            visit_or_error!(visitor, allocations.as_ref());
        }

        if let Some(memory_pages) = pages_hash.to_inner() {
            visit_or_error!(visitor, memory_pages.as_ref());
        }
    }

    if let Some(message_queue) = queue.hash.to_inner() {
        visit_or_error!(visitor, message_queue.as_ref());
    }

    if let Some(waitlist) = waitlist_hash.to_inner() {
        visit_or_error!(visitor, waitlist.as_ref());
    }

    if let Some(dispatch_stash) = stash_hash.to_inner() {
        visit_or_error!(visitor, dispatch_stash.as_ref());
    }

    if let Some(mailbox) = mailbox_hash.to_inner() {
        visit_or_error!(visitor, mailbox.as_ref());
    }
}

pub fn walk_block_outcome_is_empty<E>(
    visitor: &mut impl DatabaseVisitor<DbError = E>,
    block: H256,
    outcome_is_empty: bool,
) where
    E: DatabaseVisitorError,
{
    if !outcome_is_empty {
        visit_or_error!(visitor, &block.block_outcome);
    }
}

pub fn walk_memory_pages<E>(visitor: &mut impl DatabaseVisitor<DbError = E>, pages: &MemoryPages)
where
    E: DatabaseVisitorError,
{
    for memory_pages_region in pages.to_inner().into_iter().flat_map(MaybeHashOf::to_inner) {
        visit_or_error!(visitor, memory_pages_region.as_ref());
    }
}

pub fn walk_memory_pages_region<E>(
    visitor: &mut impl DatabaseVisitor<DbError = E>,
    region: &MemoryPagesRegion,
) where
    E: DatabaseVisitorError,
{
    for &page_data in region.as_inner().values() {
        visit_or_error!(visitor, page_data.as_ref());
    }
}

pub fn walk_message_queue<E>(visitor: &mut impl DatabaseVisitor<DbError = E>, queue: &MessageQueue)
where
    E: DatabaseVisitorError,
{
    for dispatch in queue.as_ref() {
        visitor.visit_payload_lookup(&dispatch.payload);
    }
}

pub fn walk_waitlist<E>(visitor: &mut impl DatabaseVisitor<DbError = E>, waitlist: &Waitlist)
where
    E: DatabaseVisitorError,
{
    for Expiring {
        value: dispatch,
        expiry: _,
    } in waitlist.as_ref().values()
    {
        visitor.visit_payload_lookup(&dispatch.payload);
    }
}

pub fn walk_mailbox<E>(visitor: &mut impl DatabaseVisitor<DbError = E>, mailbox: &Mailbox)
where
    E: DatabaseVisitorError,
{
    for &user_mailbox in mailbox.as_ref().values() {
        visit_or_error!(visitor, user_mailbox.as_ref());
    }
}

pub fn walk_user_mailbox<E>(
    visitor: &mut impl DatabaseVisitor<DbError = E>,
    user_mailbox: &UserMailbox,
) where
    E: DatabaseVisitorError,
{
    for Expiring {
        value: msg,
        expiry: _,
    } in user_mailbox.as_ref().values()
    {
        visitor.visit_payload_lookup(&msg.payload);
    }
}

pub fn walk_dispatch_stash<E>(
    visitor: &mut impl DatabaseVisitor<DbError = E>,
    stash: &DispatchStash,
) where
    E: DatabaseVisitorError,
{
    for Expiring {
        value: (dispatch, _user_id),
        expiry: _,
    } in stash.as_ref().values()
    {
        visitor.visit_payload_lookup(&dispatch.payload);
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum IntegrityVerifierError {
    /* block meta */
    BlockIsNotSynced,
    BlockIsNotPrepared,
    BlockIsNotComputed,

    /* block header */
    NoBlockHeader(H256),
    InvalidBlockParentHeight {
        parent_height: u32,
        height: u32,
    },
    InvalidParentTimestamp {
        parent_timestamp: u64,
        timestamp: u64,
    },

    /* block */
    NoBlockEvents(H256),
    NoBlockProgramStates(H256),
    NoBlockSchedule(H256),
    NoBlockOutcome(H256),
    NoBlockCommitmentQueue(H256),
    NoBlockCodesQueue(H256),
    NoPreviousNonEmptyBlock(H256),
    NoLastCommittedBatch(H256),

    /* memory */
    NoMemoryPages(HashOf<MemoryPages>),
    NoMemoryPagesRegion(HashOf<MemoryPagesRegion>),
    NoMemoryPageData(HashOf<PageBuf>),

    /* code */
    NoCodeValid,
    CodeIsNotValid,
    NoOriginalCode,
    NoInstrumentedCode(CodeId),
    NoCodeMetadata,
    InvalidCodeLenInMetadata {
        metadata_len: u32,
        original_len: u32,
    },

    /* rest */
    NoMessageQueue(HashOf<MessageQueue>),
    NoWaitlist(HashOf<Waitlist>),
    NoDispatchStash(HashOf<DispatchStash>),
    NoMailbox(HashOf<Mailbox>),
    NoUserMailbox(HashOf<UserMailbox>),
    NoAllocations(HashOf<Allocations>),
    NoPayload,
    NoProgramState(H256),
}

impl DatabaseVisitorError for IntegrityVerifierError {
    fn no_block_header(block: H256) -> Self {
        Self::NoBlockHeader(block)
    }

    fn no_block_events(block: H256) -> Self {
        Self::NoBlockEvents(block)
    }

    fn no_block_program_states(block: H256) -> Self {
        Self::NoBlockProgramStates(block)
    }

    fn no_block_schedule(block: H256) -> Self {
        Self::NoBlockSchedule(block)
    }

    fn no_block_outcome(block: H256) -> Self {
        Self::NoBlockOutcome(block)
    }

    fn no_block_commitment_queue(block: H256) -> Self {
        Self::NoBlockCommitmentQueue(block)
    }

    fn no_block_codes_queue(block: H256) -> Self {
        Self::NoBlockCodesQueue(block)
    }

    fn no_previous_non_empty_block(block: H256) -> Self {
        Self::NoPreviousNonEmptyBlock(block)
    }

    fn no_last_committed_batch(block: H256) -> Self {
        Self::NoLastCommittedBatch(block)
    }

    fn no_memory_pages(hash: HashOf<MemoryPages>) -> Self {
        Self::NoMemoryPages(hash)
    }

    fn no_memory_pages_region(hash: HashOf<MemoryPagesRegion>) -> Self {
        Self::NoMemoryPagesRegion(hash)
    }

    fn no_page_data(hash: HashOf<PageBuf>) -> Self {
        Self::NoMemoryPageData(hash)
    }

    fn no_message_queue(hash: HashOf<MessageQueue>) -> Self {
        Self::NoMessageQueue(hash)
    }

    fn no_waitlist(hash: HashOf<Waitlist>) -> Self {
        Self::NoWaitlist(hash)
    }

    fn no_dispatch_stash(hash: HashOf<DispatchStash>) -> Self {
        Self::NoDispatchStash(hash)
    }

    fn no_mailbox(hash: HashOf<Mailbox>) -> Self {
        Self::NoMailbox(hash)
    }

    fn no_user_mailbox(hash: HashOf<UserMailbox>) -> Self {
        Self::NoUserMailbox(hash)
    }

    fn no_allocations(hash: HashOf<Allocations>) -> Self {
        Self::NoAllocations(hash)
    }

    fn no_program_state(hash: H256) -> Self {
        Self::NoProgramState(hash)
    }
}

pub struct IntegrityVerifier {
    db: Database,
    errors: Vec<IntegrityVerifierError>,
}

impl IntegrityVerifier {
    pub fn new(db: Database) -> Self {
        Self {
            db,
            errors: Vec::new(),
        }
    }

    pub fn verify_chain(
        mut self,
        head: H256,
        bottom: H256,
    ) -> Result<(), Vec<IntegrityVerifierError>> {
        self.visit_chain(head, bottom);

        #[cfg(debug_assertions)]
        {
            self.errors
                .clone()
                .into_iter()
                .fold(HashSet::new(), |mut set, error| {
                    assert!(set.insert(error), "Duplicate error: {error:?}");
                    set
                });
        }

        if self.errors.is_empty() {
            Ok(())
        } else {
            Err(self.errors)
        }
    }
}

impl DatabaseVisitor for IntegrityVerifier {
    type DbError = IntegrityVerifierError;

    fn db(&self) -> &dyn DatabaseVisitorStorage {
        &self.db
    }

    fn on_db_error(&mut self, error: Self::DbError) {
        self.errors.push(error);
    }

    fn visit_block_meta(&mut self, meta: &BlockMeta) {
        if !meta.synced {
            self.errors.push(IntegrityVerifierError::BlockIsNotSynced);
        }
        if !meta.prepared {
            self.errors.push(IntegrityVerifierError::BlockIsNotPrepared);
        }
        if !meta.computed {
            self.errors.push(IntegrityVerifierError::BlockIsNotComputed);
        }
    }

    fn visit_block_header(&mut self, header: BlockHeader) {
        // TODO: we might want to mention its a parent
        let Some(parent_header) = self.db().block_header(header.parent_hash) else {
            self.errors
                .push(IntegrityVerifierError::NoBlockHeader(header.parent_hash));
            return;
        };

        if parent_header.height + 1 != header.height {
            self.errors
                .push(IntegrityVerifierError::InvalidBlockParentHeight {
                    parent_height: parent_header.height,
                    height: header.height,
                });
        }

        if parent_header.timestamp > header.timestamp {
            self.errors
                .push(IntegrityVerifierError::InvalidParentTimestamp {
                    parent_timestamp: parent_header.timestamp,
                    timestamp: header.timestamp,
                });
        }
    }

    fn visit_block_events(&mut self, _events: &[BlockEvent]) {
        // TODO: verification might be required for events
    }

    fn visit_block_commitment_queue(&mut self, _queue: &VecDeque<H256>) {
        // TODO: verify
    }

    fn visit_block_codes_queue(&mut self, queue: &VecDeque<CodeId>) {
        for &code in queue {
            if let Some(valid) = self.db().code_valid(code) {
                if !valid {
                    self.errors.push(IntegrityVerifierError::CodeIsNotValid);
                }
            } else {
                self.errors.push(IntegrityVerifierError::NoCodeValid);
            };

            let original_code = self.db().original_code(code);
            if original_code.is_none() {
                self.errors.push(IntegrityVerifierError::NoOriginalCode)
            }

            if self
                .db()
                .instrumented_code(ethexe_runtime_common::VERSION, code)
                .is_none()
            {
                self.errors
                    .push(IntegrityVerifierError::NoInstrumentedCode(code));
            }

            let code_metadata = self.db().code_metadata(code);
            if code_metadata.is_none() {
                self.errors.push(IntegrityVerifierError::NoCodeMetadata);
            }

            if let (Some(original_code), Some(code_metadata)) = (original_code, code_metadata)
                && code_metadata.original_code_len() != original_code.len() as u32
            {
                self.errors
                    .push(IntegrityVerifierError::InvalidCodeLenInMetadata {
                        metadata_len: code_metadata.original_code_len(),
                        original_len: original_code.len() as u32,
                    });
            }
        }
    }

    fn visit_payload_lookup(&mut self, payload_lookup: &PayloadLookup) {
        match payload_lookup {
            PayloadLookup::Direct(_payload) => {}
            PayloadLookup::Stored(hash) => {
                if self.db().payload(*hash).is_none() {
                    self.errors.push(IntegrityVerifierError::NoPayload);
                }
            }
        }
    }
}
