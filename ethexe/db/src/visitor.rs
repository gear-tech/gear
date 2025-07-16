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

pub trait DatabaseVisitor {
    type Output;

    fn visit_chain(&mut self, db: &Database, head: H256, bottom: H256) -> Self::Output;

    fn visit_block(&mut self, db: &Database, block: H256) -> Self::Output;

    fn visit_block_meta(&mut self, db: &Database, meta: &BlockMeta) -> Self::Output;

    fn visit_block_header(&mut self, db: &Database, header: BlockHeader) -> Self::Output;

    fn visit_block_codes_queue(&mut self, db: &Database, queue: &VecDeque<CodeId>) -> Self::Output;

    fn visit_program_state(&mut self, db: &Database, state: &ProgramState) -> Self::Output;

    fn visit_allocations(&mut self, db: &Database, allocations: &Allocations) -> Self::Output;

    fn visit_memory_pages(&mut self, db: &Database, memory_pages: &MemoryPages) -> Self::Output;

    fn visit_memory_pages_region(
        &mut self,
        db: &Database,
        memory_pages_region: &MemoryPagesRegion,
    ) -> Self::Output;

    fn visit_payload_lookup(
        &mut self,
        db: &Database,
        payload_lookup: &PayloadLookup,
    ) -> Self::Output;

    fn visit_message_queue(&mut self, db: &Database, queue: &MessageQueue) -> Self::Output;

    fn visit_waitlist(&mut self, db: &Database, waitlist: &Waitlist) -> Self::Output;

    fn visit_mailbox(&mut self, db: &Database, mailbox: &Mailbox) -> Self::Output;

    fn visit_user_mailbox(&mut self, db: &Database, user_mailbox: &UserMailbox) -> Self::Output;

    fn visit_dispatch_stash(&mut self, db: &Database, stash: &DispatchStash) -> Self::Output;
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

pub struct IntegrityVerifier;

impl DatabaseVisitor for IntegrityVerifier {
    type Output = Result<(), IntegrityVerifierError>;

    fn visit_chain(&mut self, db: &Database, head: H256, bottom: H256) -> Self::Output {
        let mut block = head;
        while block != bottom {
            self.visit_block(db, block)?;

            let header = db
                .block_header(block)
                .expect("`visit_block` must verify header exists");
            block = header.parent_hash;
        }

        Ok(())
    }

    fn visit_block(&mut self, db: &Database, block: H256) -> Self::Output {
        let meta = db.block_meta(block);
        self.visit_block_meta(db, &meta)?;

        let block_header = db
            .block_header(block)
            .ok_or(IntegrityVerifierError::NoBlockHeader(block))?;
        self.visit_block_header(db, block_header)?;

        let _events = db
            .block_events(block)
            .ok_or(IntegrityVerifierError::NoBlockEvents)?;
        // TODO: verification might be required for events

        let _commitment_queue = db
            .block_commitment_queue(block)
            .ok_or(IntegrityVerifierError::NoBlockCommitmentQueue)?;

        let codes_queue = db
            .block_codes_queue(block)
            .ok_or(IntegrityVerifierError::NoBlockCodesQueue)?;
        self.visit_block_codes_queue(db, &codes_queue)?;

        let _previous_non_empty_block = db
            .previous_not_empty_block(block)
            .ok_or(IntegrityVerifierError::NoPreviousNonEmptyBlock)?;

        let _last_committed_batch = db
            .last_committed_batch(block)
            .ok_or(IntegrityVerifierError::NoLastCommittedBatch)?;

        let program_states = db
            .block_program_states(block)
            .ok_or(IntegrityVerifierError::NoBlockProgramStates)?;
        for (_program_id, state) in program_states {
            let program_state = db
                .read_state(state.hash)
                .ok_or(IntegrityVerifierError::NoProgramState)?;
            // TODO: verify state.cached_queue_size
            self.visit_program_state(db, &program_state)?;
        }

        let _schedule = db
            .block_schedule(block)
            .ok_or(IntegrityVerifierError::NoBlockSchedule)?;
        // TODO: verification might be required for schedule

        let block_outcome_is_empty = db
            .block_outcome_is_empty(block)
            .ok_or(IntegrityVerifierError::NoBlockOutcome)?;
        if !block_outcome_is_empty {
            let _outcome = db
                .block_outcome(block)
                .ok_or(IntegrityVerifierError::NoBlockOutcome)?;
            // TODO: verification required for codes queue
        }

        Ok(())
    }

    fn visit_block_meta(&mut self, _db: &Database, meta: &BlockMeta) -> Self::Output {
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

    fn visit_block_header(&mut self, db: &Database, header: BlockHeader) -> Self::Output {
        let parent_header = db
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

    fn visit_block_codes_queue(&mut self, db: &Database, queue: &VecDeque<CodeId>) -> Self::Output {
        for &code in queue {
            let valid = db
                .code_valid(code)
                .ok_or(IntegrityVerifierError::NoCodeValid)?;
            if !valid {
                return Err(IntegrityVerifierError::CodeIsNotValid);
            }

            let original_code = db
                .original_code(code)
                .ok_or(IntegrityVerifierError::NoOriginalCode)?;

            let _instrumented_code = db
                .instrumented_code(ethexe_runtime_common::VERSION, code)
                .ok_or(IntegrityVerifierError::NoInstrumentedCode(code))?;

            let code_metadata = db
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

    fn visit_program_state(&mut self, db: &Database, state: &ProgramState) -> Self::Output {
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
                let allocations = db
                    .read_allocations(allocations)
                    .ok_or(IntegrityVerifierError::NoAllocations)?;
                self.visit_allocations(db, &allocations)?;
            }

            if let Some(pages) = pages_hash.to_inner() {
                let pages = db
                    .read_pages(pages)
                    .ok_or(IntegrityVerifierError::NoMemoryPages)?;
                self.visit_memory_pages(db, &pages)?;
            }
        }

        if let Some(queue) = queue.hash.to_inner() {
            let queue = db
                .read_queue(queue)
                .ok_or(IntegrityVerifierError::NoMessageQueue)?;
            self.visit_message_queue(db, &queue)?;
        }

        if let Some(waitlist) = waitlist_hash.to_inner() {
            let waitlist = db
                .read_waitlist(waitlist)
                .ok_or(IntegrityVerifierError::NoWaitlist)?;
            self.visit_waitlist(db, &waitlist)?;
        }

        if let Some(stash) = stash_hash.to_inner() {
            let stash = db
                .read_stash(stash)
                .ok_or(IntegrityVerifierError::NoDispatchStash)?;
            self.visit_dispatch_stash(db, &stash)?;
        }

        if let Some(mailbox) = mailbox_hash.to_inner() {
            let mailbox = db
                .read_mailbox(mailbox)
                .ok_or(IntegrityVerifierError::NoMailbox)?;
            self.visit_mailbox(db, &mailbox)?;
        }

        Ok(())
    }

    fn visit_allocations(&mut self, _db: &Database, _allocations: &Allocations) -> Self::Output {
        Ok(())
    }

    fn visit_memory_pages(&mut self, db: &Database, memory_pages: &MemoryPages) -> Self::Output {
        for pages_region in memory_pages
            .to_inner()
            .into_iter()
            .flat_map(MaybeHashOf::to_inner)
        {
            let pages_region = db
                .read_pages_region(pages_region)
                .ok_or(IntegrityVerifierError::NoMemoryPagesRegion)?;
            self.visit_memory_pages_region(db, &pages_region)?;
        }

        Ok(())
    }

    fn visit_memory_pages_region(
        &mut self,
        db: &Database,
        memory_pages_region: &MemoryPagesRegion,
    ) -> Self::Output {
        for &page_buf_hash in memory_pages_region.as_inner().values() {
            let _page_data = db
                .read_page_data(page_buf_hash)
                .ok_or(IntegrityVerifierError::NoMemoryPageData)?;
        }

        Ok(())
    }

    fn visit_payload_lookup(
        &mut self,
        db: &Database,
        payload_lookup: &PayloadLookup,
    ) -> Self::Output {
        match payload_lookup {
            PayloadLookup::Direct(_payload) => {}
            PayloadLookup::Stored(hash) => {
                let _payload = db
                    .read_payload(*hash)
                    .ok_or(IntegrityVerifierError::NoPayload)?;
            }
        }

        Ok(())
    }

    fn visit_message_queue(&mut self, db: &Database, queue: &MessageQueue) -> Self::Output {
        for dispatch in queue.as_ref() {
            self.visit_payload_lookup(db, &dispatch.payload)?;
        }

        Ok(())
    }

    fn visit_waitlist(&mut self, db: &Database, waitlist: &Waitlist) -> Self::Output {
        for Expiring {
            value: dispatch,
            expiry: _,
        } in waitlist.as_ref().values()
        {
            self.visit_payload_lookup(db, &dispatch.payload)?;
        }

        Ok(())
    }

    fn visit_mailbox(&mut self, db: &Database, mailbox: &Mailbox) -> Self::Output {
        for &user_mailbox in mailbox.as_ref().values() {
            let user_mailbox = db
                .read_user_mailbox(user_mailbox)
                .ok_or(IntegrityVerifierError::NoUserMailbox)?;
            self.visit_user_mailbox(db, &user_mailbox)?;
        }

        Ok(())
    }

    fn visit_user_mailbox(&mut self, db: &Database, user_mailbox: &UserMailbox) -> Self::Output {
        for Expiring {
            value: msg,
            expiry: _,
        } in user_mailbox.as_ref().values()
        {
            self.visit_payload_lookup(db, &msg.payload)?;
        }

        Ok(())
    }

    fn visit_dispatch_stash(&mut self, db: &Database, stash: &DispatchStash) -> Self::Output {
        for Expiring {
            value: (dispatch, _user_id),
            expiry: _,
        } in stash.as_ref().values()
        {
            self.visit_payload_lookup(db, &dispatch.payload)?;
        }

        Ok(())
    }
}
