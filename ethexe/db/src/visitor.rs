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
    BlockHeader, BlockMeta,
    db::{BlockMetaStorageRead, CodesStorageRead, OnChainStorageRead},
};
use ethexe_runtime_common::state::{
    ActiveProgram, Allocations, DispatchStash, Expiring, Mailbox, MaybeHashOf, MemoryPages,
    MemoryPagesRegion, MessageQueue, PayloadLookup, Program, ProgramState, Storage, UserMailbox,
    Waitlist,
};
use gprimitives::{CodeId, H256};
use std::collections::VecDeque;

pub trait DatabaseVisitorStorage:
    OnChainStorageRead + BlockMetaStorageRead + CodesStorageRead + Storage
{
}

impl<T: OnChainStorageRead + BlockMetaStorageRead + CodesStorageRead + Storage>
    DatabaseVisitorStorage for T
{
}

pub trait DatabaseVisitor: Sized {
    type Error: DatabaseVisitorError;

    fn db(&self) -> &dyn DatabaseVisitorStorage;

    fn visit_chain(&mut self, head: H256, bottom: H256) -> Result<(), Self::Error> {
        walk_chain(self, head, bottom)
    }

    fn visit_block(&mut self, block: H256) -> Result<(), Self::Error> {
        walk_block(self, block)
    }

    fn visit_block_meta(&mut self, meta: &BlockMeta) -> Result<(), Self::Error>;

    fn visit_block_header(&mut self, header: BlockHeader) -> Result<(), Self::Error>;

    fn visit_block_codes_queue(&mut self, queue: &VecDeque<CodeId>) -> Result<(), Self::Error>;

    fn visit_program_state(&mut self, state: &ProgramState) -> Result<(), Self::Error> {
        walk_program_state(self, state)
    }

    fn visit_allocations(&mut self, _allocations: &Allocations) -> Result<(), Self::Error> {
        Ok(())
    }

    fn visit_memory_pages(&mut self, memory_pages: &MemoryPages) -> Result<(), Self::Error> {
        walk_memory_pages(self, memory_pages)
    }

    fn visit_memory_pages_region(
        &mut self,

        memory_pages_region: &MemoryPagesRegion,
    ) -> Result<(), Self::Error>;

    fn visit_payload_lookup(&mut self, payload_lookup: &PayloadLookup) -> Result<(), Self::Error>;

    fn visit_message_queue(&mut self, queue: &MessageQueue) -> Result<(), Self::Error> {
        walk_message_queue(self, queue)
    }

    fn visit_waitlist(&mut self, waitlist: &Waitlist) -> Result<(), Self::Error> {
        walk_waitlist(self, waitlist)
    }

    fn visit_mailbox(&mut self, mailbox: &Mailbox) -> Result<(), Self::Error> {
        walk_mailbox(self, mailbox)
    }

    fn visit_user_mailbox(&mut self, user_mailbox: &UserMailbox) -> Result<(), Self::Error> {
        walk_user_mailbox(self, user_mailbox)
    }

    fn visit_dispatch_stash(&mut self, stash: &DispatchStash) -> Result<(), Self::Error> {
        walk_dispatch_stash(self, stash)
    }
}

pub trait DatabaseVisitorError {
    /* block header */
    fn no_block_header(block: H256) -> Self;

    /* block */
    fn no_block_events() -> Self;
    fn no_block_program_states() -> Self;
    fn no_block_schedule() -> Self;
    fn no_block_outcome() -> Self;
    fn no_block_commitment_queue() -> Self;
    fn no_block_codes_queue() -> Self;
    fn no_previous_non_empty_block() -> Self;
    fn no_last_committed_batch() -> Self;

    /* memory */
    fn no_memory_pages() -> Self;
    fn no_memory_pages_region() -> Self;

    /* rest */
    fn no_message_queue() -> Self;
    fn no_waitlist() -> Self;
    fn no_dispatch_stash() -> Self;
    fn no_mailbox() -> Self;
    fn no_user_mailbox() -> Self;
    fn no_allocations() -> Self;
    fn no_program_state() -> Self;
}

pub fn walk_chain<E>(
    visitor: &mut impl DatabaseVisitor<Error = E>,

    head: H256,
    bottom: H256,
) -> Result<(), E>
where
    E: DatabaseVisitorError,
{
    let mut block = head;
    while block != bottom {
        visitor.visit_block(block)?;

        let header = visitor
            .db()
            .block_header(block)
            .ok_or(E::no_block_header(block))?;
        block = header.parent_hash;
    }

    Ok(())
}

pub fn walk_block<E>(visitor: &mut impl DatabaseVisitor<Error = E>, block: H256) -> Result<(), E>
where
    E: DatabaseVisitorError,
{
    let meta = visitor.db().block_meta(block);
    visitor.visit_block_meta(&meta)?;

    let block_header = visitor
        .db()
        .block_header(block)
        .ok_or(E::no_block_header(block))?;
    visitor.visit_block_header(block_header)?;

    let _events = visitor
        .db()
        .block_events(block)
        .ok_or(E::no_block_events())?;
    // TODO: verification might be required for events

    let _commitment_queue = visitor
        .db()
        .block_commitment_queue(block)
        .ok_or(E::no_block_commitment_queue())?;

    let codes_queue = visitor
        .db()
        .block_codes_queue(block)
        .ok_or(E::no_block_codes_queue())?;
    visitor.visit_block_codes_queue(&codes_queue)?;

    let _previous_non_empty_block = visitor
        .db()
        .previous_not_empty_block(block)
        .ok_or(E::no_previous_non_empty_block())?;

    let _last_committed_batch = visitor
        .db()
        .last_committed_batch(block)
        .ok_or(E::no_last_committed_batch())?;

    let program_states = visitor
        .db()
        .block_program_states(block)
        .ok_or(E::no_block_program_states())?;
    for (_program_id, state) in program_states {
        let program_state = visitor
            .db()
            .read_state(state.hash)
            .ok_or(E::no_program_state())?;
        // TODO: verify state.cached_queue_size
        visitor.visit_program_state(&program_state)?;
    }

    let _schedule = visitor
        .db()
        .block_schedule(block)
        .ok_or(E::no_block_schedule())?;
    // TODO: verification might be required for schedule

    let block_outcome_is_empty = visitor
        .db()
        .block_outcome_is_empty(block)
        .ok_or(E::no_block_outcome())?;
    if !block_outcome_is_empty {
        let _outcome = visitor
            .db()
            .block_outcome(block)
            .ok_or(E::no_block_outcome())?;
        // TODO: verification required for codes queue
    }

    Ok(())
}

pub fn walk_program_state<E>(
    visitor: &mut impl DatabaseVisitor<Error = E>,

    state: &ProgramState,
) -> Result<(), E>
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
            let allocations = visitor
                .db()
                .read_allocations(allocations)
                .ok_or(E::no_allocations())?;
            visitor.visit_allocations(&allocations)?;
        }

        if let Some(pages) = pages_hash.to_inner() {
            let pages = visitor.db().read_pages(pages).ok_or(E::no_memory_pages())?;
            visitor.visit_memory_pages(&pages)?;
        }
    }

    if let Some(queue) = queue.hash.to_inner() {
        let queue = visitor
            .db()
            .read_queue(queue)
            .ok_or(E::no_message_queue())?;
        visitor.visit_message_queue(&queue)?;
    }

    if let Some(waitlist) = waitlist_hash.to_inner() {
        let waitlist = visitor
            .db()
            .read_waitlist(waitlist)
            .ok_or(E::no_waitlist())?;
        visitor.visit_waitlist(&waitlist)?;
    }

    if let Some(stash) = stash_hash.to_inner() {
        let stash = visitor
            .db()
            .read_stash(stash)
            .ok_or(E::no_dispatch_stash())?;
        visitor.visit_dispatch_stash(&stash)?;
    }

    if let Some(mailbox) = mailbox_hash.to_inner() {
        let mailbox = visitor.db().read_mailbox(mailbox).ok_or(E::no_mailbox())?;
        visitor.visit_mailbox(&mailbox)?;
    }

    Ok(())
}

pub fn walk_memory_pages<E>(
    visitor: &mut impl DatabaseVisitor<Error = E>,

    pages: &MemoryPages,
) -> Result<(), E>
where
    E: DatabaseVisitorError,
{
    for pages_region in pages.to_inner().into_iter().flat_map(MaybeHashOf::to_inner) {
        let pages_region = visitor
            .db()
            .read_pages_region(pages_region)
            .ok_or(E::no_memory_pages_region())?;
        visitor.visit_memory_pages_region(&pages_region)?;
    }

    Ok(())
}

pub fn walk_message_queue<E>(
    visitor: &mut impl DatabaseVisitor<Error = E>,

    queue: &MessageQueue,
) -> Result<(), E>
where
    E: DatabaseVisitorError,
{
    for dispatch in queue.as_ref() {
        visitor.visit_payload_lookup(&dispatch.payload)?;
    }

    Ok(())
}

pub fn walk_waitlist<E>(
    visitor: &mut impl DatabaseVisitor<Error = E>,

    waitlist: &Waitlist,
) -> Result<(), E>
where
    E: DatabaseVisitorError,
{
    for Expiring {
        value: dispatch,
        expiry: _,
    } in waitlist.as_ref().values()
    {
        visitor.visit_payload_lookup(&dispatch.payload)?;
    }

    Ok(())
}

pub fn walk_mailbox<E>(
    visitor: &mut impl DatabaseVisitor<Error = E>,

    mailbox: &Mailbox,
) -> Result<(), E>
where
    E: DatabaseVisitorError,
{
    for &user_mailbox in mailbox.as_ref().values() {
        let user_mailbox = visitor
            .db()
            .read_user_mailbox(user_mailbox)
            .ok_or(E::no_user_mailbox())?;
        visitor.visit_user_mailbox(&user_mailbox)?;
    }

    Ok(())
}

pub fn walk_user_mailbox<E>(
    visitor: &mut impl DatabaseVisitor<Error = E>,

    user_mailbox: &UserMailbox,
) -> Result<(), E>
where
    E: DatabaseVisitorError,
{
    for Expiring {
        value: msg,
        expiry: _,
    } in user_mailbox.as_ref().values()
    {
        visitor.visit_payload_lookup(&msg.payload)?;
    }

    Ok(())
}

pub fn walk_dispatch_stash<E>(
    visitor: &mut impl DatabaseVisitor<Error = E>,

    stash: &DispatchStash,
) -> Result<(), E>
where
    E: DatabaseVisitorError,
{
    for Expiring {
        value: (dispatch, _user_id),
        expiry: _,
    } in stash.as_ref().values()
    {
        visitor.visit_payload_lookup(&dispatch.payload)?;
    }

    Ok(())
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
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
    NoBlockEvents,
    NoBlockProgramStates,
    NoBlockSchedule,
    NoBlockOutcome,
    NoBlockCommitmentQueue,
    NoBlockCodesQueue,
    NoPreviousNonEmptyBlock,
    NoLastCommittedBatch,

    /* memory */
    NoMemoryPages,
    NoMemoryPagesRegion,
    NoMemoryPageData,

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
    NoMessageQueue,
    NoWaitlist,
    NoDispatchStash,
    NoMailbox,
    NoUserMailbox,
    NoAllocations,
    NoPayload,
    NoProgramState,
}

impl DatabaseVisitorError for IntegrityVerifierError {
    fn no_block_header(block: H256) -> Self {
        Self::NoBlockHeader(block)
    }

    fn no_block_events() -> Self {
        Self::NoBlockEvents
    }

    fn no_block_program_states() -> Self {
        Self::NoBlockProgramStates
    }

    fn no_block_schedule() -> Self {
        Self::NoBlockSchedule
    }

    fn no_block_outcome() -> Self {
        Self::NoBlockOutcome
    }

    fn no_block_commitment_queue() -> Self {
        Self::NoBlockCommitmentQueue
    }

    fn no_block_codes_queue() -> Self {
        Self::NoBlockCodesQueue
    }

    fn no_previous_non_empty_block() -> Self {
        Self::NoPreviousNonEmptyBlock
    }

    fn no_last_committed_batch() -> Self {
        Self::NoLastCommittedBatch
    }

    fn no_memory_pages() -> Self {
        Self::NoMemoryPages
    }

    fn no_memory_pages_region() -> Self {
        Self::NoMemoryPagesRegion
    }

    fn no_message_queue() -> Self {
        Self::NoMessageQueue
    }

    fn no_waitlist() -> Self {
        Self::NoWaitlist
    }

    fn no_dispatch_stash() -> Self {
        Self::NoDispatchStash
    }

    fn no_mailbox() -> Self {
        Self::NoMailbox
    }

    fn no_user_mailbox() -> Self {
        Self::NoUserMailbox
    }

    fn no_allocations() -> Self {
        Self::NoAllocations
    }

    fn no_program_state() -> Self {
        Self::NoProgramState
    }
}

pub struct IntegrityVerifier<'a>(pub &'a Database);

impl DatabaseVisitor for IntegrityVerifier<'_> {
    type Error = IntegrityVerifierError;

    fn db(&self) -> &dyn DatabaseVisitorStorage {
        self.0
    }

    fn visit_block_meta(&mut self, meta: &BlockMeta) -> Result<(), Self::Error> {
        if !meta.synced {
            return Err(IntegrityVerifierError::BlockIsNotSynced);
        }
        if !meta.prepared {
            return Err(IntegrityVerifierError::BlockIsNotPrepared);
        }
        if !meta.computed {
            return Err(IntegrityVerifierError::BlockIsNotComputed);
        }

        Ok(())
    }

    fn visit_block_header(&mut self, header: BlockHeader) -> Result<(), Self::Error> {
        let parent_header = self
            .db()
            .block_header(header.parent_hash)
            .ok_or(IntegrityVerifierError::NoBlockHeader(header.parent_hash))?; // TODO: we might want to mention its a parent

        if parent_header.height + 1 != header.height {
            return Err(IntegrityVerifierError::InvalidBlockParentHeight {
                parent_height: parent_header.height,
                height: header.height,
            });
        }

        if parent_header.timestamp > header.timestamp {
            return Err(IntegrityVerifierError::InvalidParentTimestamp {
                parent_timestamp: parent_header.timestamp,
                timestamp: header.timestamp,
            });
        }

        Ok(())
    }

    fn visit_block_codes_queue(&mut self, queue: &VecDeque<CodeId>) -> Result<(), Self::Error> {
        for &code in queue {
            let valid = self
                .db()
                .code_valid(code)
                .ok_or(IntegrityVerifierError::NoCodeValid)?;
            if !valid {
                return Err(IntegrityVerifierError::CodeIsNotValid);
            }

            let original_code = self
                .db()
                .original_code(code)
                .ok_or(IntegrityVerifierError::NoOriginalCode)?;

            let _instrumented_code = self
                .db()
                .instrumented_code(ethexe_runtime_common::VERSION, code)
                .ok_or(IntegrityVerifierError::NoInstrumentedCode(code))?;

            let code_metadata = self
                .db()
                .code_metadata(code)
                .ok_or(IntegrityVerifierError::NoCodeMetadata)?;
            if code_metadata.original_code_len() != original_code.len() as u32 {
                return Err(IntegrityVerifierError::InvalidCodeLenInMetadata {
                    metadata_len: code_metadata.original_code_len(),
                    original_len: original_code.len() as u32,
                });
            }
        }

        Ok(())
    }

    fn visit_memory_pages_region(
        &mut self,

        memory_pages_region: &MemoryPagesRegion,
    ) -> Result<(), Self::Error> {
        for &page_buf_hash in memory_pages_region.as_inner().values() {
            let _page_data = self
                .db()
                .read_page_data(page_buf_hash)
                .ok_or(IntegrityVerifierError::NoMemoryPageData)?;
        }

        Ok(())
    }

    fn visit_payload_lookup(&mut self, payload_lookup: &PayloadLookup) -> Result<(), Self::Error> {
        match payload_lookup {
            PayloadLookup::Direct(_payload) => {}
            PayloadLookup::Stored(hash) => {
                let _payload = self
                    .db()
                    .read_payload(*hash)
                    .ok_or(IntegrityVerifierError::NoPayload)?;
            }
        }

        Ok(())
    }
}
