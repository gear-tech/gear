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
    BlockHeader, BlockMeta, Digest, ProgramStates, Schedule, ScheduledTask, StateHashWithQueueSize,
    db::{BlockMetaStorageRead, CodesStorageRead, OnChainStorageRead},
    events::BlockEvent,
    gear::StateTransition,
};
use ethexe_runtime_common::state::{
    ActiveProgram, Allocations, DispatchStash, Expiring, HashOf, Mailbox, MaybeHashOf, MemoryPages,
    MemoryPagesRegion, MessageQueue, PayloadLookup, Program, ProgramState, Storage, UserMailbox,
    Waitlist,
};
use gear_core::{buffer::Payload, memory::PageBuf};
use gprimitives::{ActorId, CodeId, H256};
use std::collections::{BTreeSet, HashSet, VecDeque};

pub trait DatabaseVisitorStorage:
    OnChainStorageRead + BlockMetaStorageRead + CodesStorageRead + Storage
{
}

impl<T: OnChainStorageRead + BlockMetaStorageRead + CodesStorageRead + Storage>
    DatabaseVisitorStorage for T
{
}

pub trait DatabaseVisitor: Sized {
    fn db(&self) -> &dyn DatabaseVisitorStorage;

    fn on_db_error(&mut self, error: DatabaseVisitorError);

    fn visit_chain(&mut self, head: H256, bottom: H256) {
        walk_chain(self, head, bottom)
    }

    fn visit_block(&mut self, block: H256) {
        walk_block(self, block)
    }

    fn visit_block_meta(&mut self, _block: H256, _meta: &BlockMeta) {}

    fn visit_block_header(&mut self, _block: H256, _header: BlockHeader) {}

    fn visit_block_events(&mut self, _block: H256, _events: &[BlockEvent]) {}

    fn visit_block_commitment_queue(&mut self, _block: H256, _queue: &VecDeque<H256>) {}

    fn visit_block_codes_queue(&mut self, _block: H256, queue: &VecDeque<CodeId>) {
        walk_block_codes_queue(self, queue)
    }

    fn visit_code_id(&mut self, _code_id: CodeId) {}

    fn visit_program_id(&mut self, _program_id: ActorId) {}

    fn visit_previous_non_empty_block(&mut self, _block: H256, _previous_non_empty_block: H256) {}

    fn visit_last_committed_batch(&mut self, _block: H256, _batch: Digest) {}

    fn visit_block_program_states(&mut self, _block: H256, program_states: &ProgramStates) {
        walk_block_program_states(self, program_states)
    }

    fn visit_program_state(&mut self, state: &ProgramState) {
        walk_program_state(self, state)
    }

    fn visit_block_schedule(&mut self, block: H256, schedule: &Schedule) {
        walk_block_schedule(self, block, schedule)
    }

    fn visit_block_schedule_tasks(
        &mut self,
        _block: H256,
        _height: u32,
        tasks: &BTreeSet<ScheduledTask>,
    ) {
        walk_block_schedule_tasks(self, tasks)
    }

    fn visit_scheduled_task(&mut self, task: &ScheduledTask) {
        walk_scheduled_task(self, task)
    }

    fn visit_block_outcome_is_empty(&mut self, block: H256, outcome_is_empty: bool) {
        walk_block_outcome_is_empty(self, block, outcome_is_empty)
    }

    fn visit_block_outcome(&mut self, _block: H256, outcome: &[StateTransition]) {
        walk_block_outcome(self, outcome)
    }

    fn visit_state_transition(&mut self, state_transition: &StateTransition) {
        walk_state_transition(self, state_transition)
    }

    fn visit_allocations(&mut self, _allocations: &Allocations) {}

    fn visit_memory_pages(&mut self, memory_pages: &MemoryPages) {
        walk_memory_pages(self, memory_pages)
    }

    fn visit_memory_pages_region(&mut self, memory_pages_region: &MemoryPagesRegion) {
        walk_memory_pages_region(self, memory_pages_region)
    }

    fn visit_page_data(&mut self, _page_data: &[u8]) {}

    fn visit_payload_lookup(&mut self, payload_lookup: &PayloadLookup) {
        walk_payload_lookup(self, payload_lookup)
    }

    fn visit_payload(&mut self, _payload: &Payload) {}

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

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum DatabaseVisitorError {
    /* block header */
    NoBlockHeader(H256),

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
    NoPageData(HashOf<PageBuf>),

    /* rest */
    NoMessageQueue(HashOf<MessageQueue>),
    NoWaitlist(HashOf<Waitlist>),
    NoDispatchStash(HashOf<DispatchStash>),
    NoMailbox(HashOf<Mailbox>),
    NoUserMailbox(HashOf<UserMailbox>),
    NoAllocations(HashOf<Allocations>),
    NoProgramState(H256),
    NoPayload(HashOf<Payload>),
    NoProgramCodeId(ActorId),
}

macro_rules! visit_or_error {
    ($visitor:ident, $hash:ident.$element:ident) => {{
        let x = $visitor.db().$element($hash);
        if let Some(x) = x {
            paste::paste! {
                 $visitor. [< visit_ $element >] ($hash, x);
            }
        } else {
            paste::item! {
                $visitor.on_db_error(DatabaseVisitorError:: [< No $element:camel >] ($hash));
            }
        }
        x
    }};
    ($visitor:ident, &$hash:ident.$element:ident) => {{
        let x = $visitor.db().$element($hash);
        if let Some(x) = &x {
            paste::paste! {
                 $visitor. [< visit_ $element >] ($hash, x);
            }
        } else {
            paste::item! {
                $visitor.on_db_error(DatabaseVisitorError:: [< No $element:camel >] ($hash));
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
                $visitor.on_db_error(DatabaseVisitorError:: [< No $element:camel >] ($element));
            }
        }
        x
    }};
}

pub fn walk_chain(visitor: &mut impl DatabaseVisitor, head: H256, bottom: H256) {
    let mut block = head;
    while block != bottom {
        visitor.visit_block(block);

        let header = visitor.db().block_header(block);
        if let Some(header) = header {
            block = header.parent_hash;
        } else {
            visitor.on_db_error(DatabaseVisitorError::NoBlockHeader(block));
            break;
        }
    }
}

pub fn walk_block(visitor: &mut impl DatabaseVisitor, block: H256) {
    let meta = visitor.db().block_meta(block);
    visitor.visit_block_meta(block, &meta);

    visit_or_error!(visitor, block.block_header);

    visit_or_error!(visitor, &block.block_events);

    visit_or_error!(visitor, &block.block_commitment_queue);

    visit_or_error!(visitor, &block.block_codes_queue);

    visit_or_error!(visitor, block.previous_non_empty_block);

    visit_or_error!(visitor, block.last_committed_batch);

    visit_or_error!(visitor, &block.block_program_states);

    visit_or_error!(visitor, &block.block_schedule);

    let outcome_is_empty = visitor.db().block_outcome_is_empty(block);
    if let Some(outcome_is_empty) = outcome_is_empty {
        visitor.visit_block_outcome_is_empty(block, outcome_is_empty);
    } else {
        visitor.on_db_error(DatabaseVisitorError::NoBlockOutcome(block));
    }
}

pub fn walk_block_codes_queue(visitor: &mut impl DatabaseVisitor, queue: &VecDeque<CodeId>) {
    for &code in queue {
        visitor.visit_code_id(code);
    }
}

pub fn walk_program_id(visitor: &mut impl DatabaseVisitor, program_id: ActorId) {
    let Some(code_id) = visitor.db().program_code_id(program_id) else {
        visitor.on_db_error(DatabaseVisitorError::NoProgramCodeId(program_id));
        return;
    };

    visitor.visit_code_id(code_id);
}

pub fn walk_block_program_states(
    visitor: &mut impl DatabaseVisitor,
    program_states: &ProgramStates,
) {
    for StateHashWithQueueSize {
        hash: program_state,
        cached_queue_size: _,
    } in program_states.values().copied()
    {
        visit_or_error!(visitor, program_state.as_ref());
    }
}

pub fn walk_program_state(visitor: &mut impl DatabaseVisitor, state: &ProgramState) {
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

pub fn walk_block_schedule(visitor: &mut impl DatabaseVisitor, block: H256, schedule: &Schedule) {
    for (&height, tasks) in schedule {
        visitor.visit_block_schedule_tasks(block, height, tasks);
    }
}

pub fn walk_block_schedule_tasks(
    visitor: &mut impl DatabaseVisitor,
    tasks: &BTreeSet<ScheduledTask>,
) {
    for task in tasks {
        visitor.visit_scheduled_task(task);
    }
}

pub fn walk_scheduled_task(visitor: &mut impl DatabaseVisitor, task: &ScheduledTask) {
    match *task {
        ScheduledTask::PauseProgram(program_id) => {
            visitor.visit_program_id(program_id);
        }
        ScheduledTask::RemoveCode(code_id) => {
            visitor.visit_code_id(code_id);
        }
        ScheduledTask::RemoveFromMailbox((program_id, _destination), _msg_id) => {
            visitor.visit_program_id(program_id);
        }
        ScheduledTask::RemoveFromWaitlist(program_id, _) => {
            visitor.visit_program_id(program_id);
        }
        ScheduledTask::RemovePausedProgram(program_id) => {
            visitor.visit_program_id(program_id);
        }
        ScheduledTask::WakeMessage(program_id, _) => {
            visitor.visit_program_id(program_id);
        }
        ScheduledTask::SendDispatch((program_id, _msg_id)) => {
            visitor.visit_program_id(program_id);
        }
        ScheduledTask::SendUserMessage {
            message_id: _,
            to_mailbox: program_id,
        } => {
            visitor.visit_program_id(program_id);
        }
        ScheduledTask::RemoveGasReservation(program_id, _) => {
            visitor.visit_program_id(program_id);
        }
        #[allow(deprecated)]
        ScheduledTask::RemoveResumeSession(_) => unreachable!("deprecated"),
    }
}

pub fn walk_block_outcome_is_empty(
    visitor: &mut impl DatabaseVisitor,
    block: H256,
    outcome_is_empty: bool,
) {
    if !outcome_is_empty {
        visit_or_error!(visitor, &block.block_outcome);
    }
}

pub fn walk_block_outcome(visitor: &mut impl DatabaseVisitor, outcome: &[StateTransition]) {
    for transition in outcome {
        visitor.visit_state_transition(transition);
    }
}

pub fn walk_state_transition(
    visitor: &mut impl DatabaseVisitor,
    state_transition: &StateTransition,
) {
    let &StateTransition {
        actor_id,
        new_state_hash: program_state,
        exited: _,
        inheritor: _,
        value_to_receive: _,
        value_claims: _,
        messages: _,
    } = state_transition;

    visitor.visit_program_id(actor_id);

    if program_state != H256::zero() {
        visit_or_error!(visitor, program_state.as_ref());
    }
}

pub fn walk_memory_pages(visitor: &mut impl DatabaseVisitor, pages: &MemoryPages) {
    for memory_pages_region in pages.to_inner().into_iter().flat_map(MaybeHashOf::to_inner) {
        visit_or_error!(visitor, memory_pages_region.as_ref());
    }
}

pub fn walk_memory_pages_region(visitor: &mut impl DatabaseVisitor, region: &MemoryPagesRegion) {
    for &page_data in region.as_inner().values() {
        visit_or_error!(visitor, page_data.as_ref());
    }
}

pub fn walk_message_queue(visitor: &mut impl DatabaseVisitor, queue: &MessageQueue) {
    for dispatch in queue.as_ref() {
        visitor.visit_payload_lookup(&dispatch.payload);
    }
}

pub fn walk_waitlist(visitor: &mut impl DatabaseVisitor, waitlist: &Waitlist) {
    for Expiring {
        value: dispatch,
        expiry: _,
    } in waitlist.as_ref().values()
    {
        visitor.visit_payload_lookup(&dispatch.payload);
    }
}

pub fn walk_mailbox(visitor: &mut impl DatabaseVisitor, mailbox: &Mailbox) {
    for &user_mailbox in mailbox.as_ref().values() {
        visit_or_error!(visitor, user_mailbox.as_ref());
    }
}

pub fn walk_user_mailbox(visitor: &mut impl DatabaseVisitor, user_mailbox: &UserMailbox) {
    for Expiring {
        value: msg,
        expiry: _,
    } in user_mailbox.as_ref().values()
    {
        visitor.visit_payload_lookup(&msg.payload);
    }
}

pub fn walk_dispatch_stash(visitor: &mut impl DatabaseVisitor, stash: &DispatchStash) {
    for Expiring {
        value: (dispatch, _user_id),
        expiry: _,
    } in stash.as_ref().values()
    {
        visitor.visit_payload_lookup(&dispatch.payload);
    }
}

pub fn walk_payload_lookup(visitor: &mut impl DatabaseVisitor, payload_lookup: &PayloadLookup) {
    match payload_lookup {
        PayloadLookup::Direct(payload) => {
            visitor.visit_payload(payload);
        }
        PayloadLookup::Stored(payload) => {
            let payload = *payload;
            visit_or_error!(visitor, payload.as_ref());
        }
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum IntegrityVerifierError {
    DatabaseVisitor(DatabaseVisitorError),

    /* block meta */
    BlockIsNotSynced,
    BlockIsNotPrepared,
    BlockIsNotComputed,

    /* block header */
    NoParentBlockHeader(H256),
    InvalidBlockParentHeight {
        parent_height: u32,
        height: u32,
    },
    InvalidParentTimestamp {
        parent_timestamp: u64,
        timestamp: u64,
    },

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

    NoBlockHeader(H256),
    BlockScheduleHasExpiredTasks {
        block: H256,
        expiry: u32,
        tasks: usize,
    },
}

pub struct IntegrityVerifier {
    db: Database,
    visited_blocks: HashSet<H256>,
    errors: Vec<IntegrityVerifierError>,
}

impl IntegrityVerifier {
    pub fn new(db: Database) -> Self {
        Self {
            db,
            visited_blocks: HashSet::new(),
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
    fn db(&self) -> &dyn DatabaseVisitorStorage {
        &self.db
    }

    fn on_db_error(&mut self, error: DatabaseVisitorError) {
        self.errors
            .push(IntegrityVerifierError::DatabaseVisitor(error));
    }

    fn visit_block(&mut self, block: H256) {
        if self.visited_blocks.insert(block) {
            walk_block(self, block);
        }
    }

    fn visit_block_meta(&mut self, _block: H256, meta: &BlockMeta) {
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

    fn visit_block_header(&mut self, _block: H256, header: BlockHeader) {
        let Some(parent_header) = self.db().block_header(header.parent_hash) else {
            self.errors
                .push(IntegrityVerifierError::NoParentBlockHeader(
                    header.parent_hash,
                ));
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

    fn visit_block_commitment_queue(&mut self, _block: H256, queue: &VecDeque<H256>) {
        for &block in queue {
            self.visit_block(block);
        }
    }

    fn visit_code_id(&mut self, code: CodeId) {
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

    fn visit_block_schedule_tasks(
        &mut self,
        block: H256,
        height: u32,
        tasks: &BTreeSet<ScheduledTask>,
    ) {
        let Some(header) = self.db().block_header(block) else {
            self.errors
                .push(IntegrityVerifierError::NoBlockHeader(block));
            return;
        };

        if height <= header.height {
            self.errors
                .push(IntegrityVerifierError::BlockScheduleHasExpiredTasks {
                    block,
                    expiry: height,
                    tasks: tasks.len(),
                });
        }

        walk_block_schedule_tasks(self, tasks);
    }
}
