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

use ethexe_common::{
    BlockHeader, BlockMeta, Digest, ProgramStates, Schedule, ScheduledTask, StateHashWithQueueSize,
    db::{BlockMetaStorageRead, BlockOutcome, CodesStorageRead, OnChainStorageRead},
    events::BlockEvent,
    gear::StateTransition,
};
use ethexe_runtime_common::state::{
    ActiveProgram, Allocations, DispatchStash, Expiring, HashOf, Mailbox, MaybeHashOf, MemoryPages,
    MemoryPagesRegion, MessageQueue, MessageQueueHashWithSize, PayloadLookup, Program,
    ProgramState, Storage, UserMailbox, Waitlist,
};
use gear_core::{
    buffer::Payload,
    code::{CodeMetadata, InstrumentedCode},
    memory::PageBuf,
};
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

    fn visit_chain(&mut self, _head: H256, _bottom: H256) {}

    fn visit_block(&mut self, _block: H256) {}

    fn visit_block_meta(&mut self, _block: H256, _meta: &BlockMeta) {}

    fn visit_block_header(&mut self, _block: H256, _header: BlockHeader) {}

    fn visit_block_events(&mut self, _block: H256, _events: &[BlockEvent]) {}

    fn visit_block_commitment_queue(&mut self, _block: H256, _queue: &VecDeque<H256>) {}

    fn visit_block_codes_queue(&mut self, _block: H256, _queue: &VecDeque<CodeId>) {}

    fn visit_code_id(&mut self, _code_id: CodeId) {}

    fn visit_code_valid(&mut self, _code_id: CodeId, _code_valid: bool) {}

    fn visit_original_code(&mut self, _original_code: &[u8]) {}

    fn visit_instrumented_code(&mut self, _code_id: CodeId, _instrumented_code: &InstrumentedCode) {
    }

    fn visit_code_metadata(&mut self, _code_id: CodeId, _metadata: &CodeMetadata) {}

    fn visit_program_id(&mut self, _program_id: ActorId) {}

    fn visit_previous_non_empty_block(&mut self, _block: H256, _previous_non_empty_block: H256) {}

    fn visit_last_committed_batch(&mut self, _block: H256, _batch: Digest) {}

    fn visit_block_program_states(&mut self, _block: H256, _program_states: &ProgramStates) {}

    fn visit_program_state(&mut self, _state: &ProgramState) {}

    fn visit_block_schedule(&mut self, _block: H256, _schedule: &Schedule) {}

    fn visit_block_schedule_tasks(
        &mut self,
        _block: H256,
        _height: u32,
        _tasks: &BTreeSet<ScheduledTask>,
    ) {
    }

    fn visit_scheduled_task(&mut self, _task: &ScheduledTask) {}

    fn visit_block_outcome(&mut self, _block: H256, _outcome: &BlockOutcome) {}

    fn visit_state_transition(&mut self, _state_transition: &StateTransition) {}

    fn visit_allocations(&mut self, _allocations: &Allocations) {}

    fn visit_memory_pages(&mut self, _memory_pages: &MemoryPages) {}

    fn visit_memory_pages_region(&mut self, _memory_pages_region: &MemoryPagesRegion) {}

    fn visit_page_data(&mut self, _page_data: &[u8]) {}

    fn visit_payload_lookup(&mut self, _payload_lookup: &PayloadLookup) {}

    fn visit_payload(&mut self, _payload: &Payload) {}

    fn visit_message_queue_hash_with_size(
        &mut self,
        _queue_hash_with_size: MessageQueueHashWithSize,
    ) {
    }

    fn visit_message_queue(&mut self, _queue: &MessageQueue) {}

    fn visit_waitlist(&mut self, _waitlist: &Waitlist) {}

    fn visit_mailbox(&mut self, _mailbox: &Mailbox) {}

    fn visit_user_mailbox(&mut self, _user_mailbox: &UserMailbox) {}

    fn visit_dispatch_stash(&mut self, _stash: &DispatchStash) {}
}

impl<T> DatabaseVisitor for &mut T
where
    T: DatabaseVisitor,
{
    fn db(&self) -> &dyn DatabaseVisitorStorage {
        T::db(self)
    }

    fn on_db_error(&mut self, error: DatabaseVisitorError) {
        T::on_db_error(self, error)
    }

    fn visit_chain(&mut self, head: H256, _bottom: H256) {
        T::visit_chain(self, head, _bottom)
    }

    fn visit_block(&mut self, block: H256) {
        T::visit_block(self, block)
    }

    fn visit_block_meta(&mut self, _block: H256, _meta: &BlockMeta) {
        T::visit_block_meta(self, _block, _meta)
    }

    fn visit_block_header(&mut self, _block: H256, _header: BlockHeader) {
        T::visit_block_header(self, _block, _header)
    }

    fn visit_block_events(&mut self, _block: H256, _events: &[BlockEvent]) {
        T::visit_block_events(self, _block, _events)
    }

    fn visit_block_commitment_queue(&mut self, _block: H256, _queue: &VecDeque<H256>) {
        T::visit_block_commitment_queue(self, _block, _queue)
    }

    fn visit_block_codes_queue(&mut self, _block: H256, queue: &VecDeque<CodeId>) {
        T::visit_block_codes_queue(self, _block, queue)
    }

    fn visit_code_id(&mut self, code_id: CodeId) {
        T::visit_code_id(self, code_id)
    }

    fn visit_code_valid(&mut self, _code_id: CodeId, _code_valid: bool) {
        T::visit_code_valid(self, _code_id, _code_valid)
    }

    fn visit_original_code(&mut self, _original_code: &[u8]) {
        T::visit_original_code(self, _original_code)
    }

    fn visit_instrumented_code(&mut self, _code_id: CodeId, _instrumented_code: &InstrumentedCode) {
    }

    fn visit_code_metadata(&mut self, _code_id: CodeId, _metadata: &CodeMetadata) {
        T::visit_code_metadata(self, _code_id, _metadata)
    }

    fn visit_program_id(&mut self, _program_id: ActorId) {
        T::visit_program_id(self, _program_id)
    }

    fn visit_previous_non_empty_block(&mut self, _block: H256, _previous_non_empty_block: H256) {
        T::visit_previous_non_empty_block(self, _block, _previous_non_empty_block)
    }

    fn visit_last_committed_batch(&mut self, _block: H256, _batch: Digest) {
        T::visit_last_committed_batch(self, _block, _batch)
    }

    fn visit_block_program_states(&mut self, _block: H256, program_states: &ProgramStates) {
        T::visit_block_program_states(self, _block, program_states)
    }

    fn visit_program_state(&mut self, state: &ProgramState) {
        T::visit_program_state(self, state)
    }

    fn visit_block_schedule(&mut self, block: H256, schedule: &Schedule) {
        T::visit_block_schedule(self, block, schedule)
    }

    fn visit_block_schedule_tasks(
        &mut self,
        _block: H256,
        _height: u32,
        tasks: &BTreeSet<ScheduledTask>,
    ) {
        T::visit_block_schedule_tasks(self, _block, _height, tasks)
    }

    fn visit_scheduled_task(&mut self, task: &ScheduledTask) {
        T::visit_scheduled_task(self, task)
    }

    fn visit_block_outcome(&mut self, _block: H256, outcome: &BlockOutcome) {
        T::visit_block_outcome(self, _block, outcome)
    }

    fn visit_state_transition(&mut self, state_transition: &StateTransition) {
        T::visit_state_transition(self, state_transition)
    }

    fn visit_allocations(&mut self, _allocations: &Allocations) {
        T::visit_allocations(self, _allocations)
    }

    fn visit_memory_pages(&mut self, memory_pages: &MemoryPages) {
        T::visit_memory_pages(self, memory_pages)
    }

    fn visit_memory_pages_region(&mut self, memory_pages_region: &MemoryPagesRegion) {
        T::visit_memory_pages_region(self, memory_pages_region)
    }

    fn visit_page_data(&mut self, _page_data: &[u8]) {
        T::visit_page_data(self, _page_data)
    }

    fn visit_payload_lookup(&mut self, payload_lookup: &PayloadLookup) {
        T::visit_payload_lookup(self, payload_lookup)
    }

    fn visit_payload(&mut self, _payload: &Payload) {
        T::visit_payload(self, _payload)
    }

    fn visit_message_queue_hash_with_size(
        &mut self,
        queue_hash_with_size: MessageQueueHashWithSize,
    ) {
        T::visit_message_queue_hash_with_size(self, queue_hash_with_size)
    }

    fn visit_message_queue(&mut self, queue: &MessageQueue) {
        T::visit_message_queue(self, queue)
    }

    fn visit_waitlist(&mut self, waitlist: &Waitlist) {
        T::visit_waitlist(self, waitlist)
    }

    fn visit_mailbox(&mut self, mailbox: &Mailbox) {
        T::visit_mailbox(self, mailbox)
    }

    fn visit_user_mailbox(&mut self, user_mailbox: &UserMailbox) {
        T::visit_user_mailbox(self, user_mailbox)
    }

    fn visit_dispatch_stash(&mut self, stash: &DispatchStash) {
        T::visit_dispatch_stash(self, stash)
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum DatabaseVisitorError {
    /* block */
    NoBlockHeader(H256),
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

    /* code */
    NoCodeValid(CodeId),
    NoOriginalCode(CodeId),
    NoInstrumentedCode(CodeId),
    NoCodeMetadata(CodeId),

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

pub struct DatabaseWalker<V> {
    visitor: V,
    visited_blocks: HashSet<H256>,
}

impl<V> DatabaseWalker<V>
where
    V: DatabaseVisitor,
{
    pub fn new(visitor: V) -> Self {
        Self {
            visitor,
            visited_blocks: HashSet::new(),
        }
    }

    pub fn into_visitor(self) -> V {
        self.visitor
    }

    fn walk_chain(&mut self, head: H256, bottom: H256) {
        let mut block = head;
        loop {
            self.visit_block(block);

            if block == bottom {
                break;
            }

            let header = self.db().block_header(block);
            if let Some(header) = header {
                block = header.parent_hash;
            } else {
                self.on_db_error(DatabaseVisitorError::NoBlockHeader(block));
                break;
            }
        }
    }

    fn walk_block(&mut self, block: H256) {
        let meta = self.db().block_meta(block);
        self.visit_block_meta(block, &meta);

        visit_or_error!(self, block.block_header);

        visit_or_error!(self, &block.block_events);

        visit_or_error!(self, &block.block_commitment_queue);

        visit_or_error!(self, &block.block_codes_queue);

        visit_or_error!(self, block.previous_non_empty_block);

        visit_or_error!(self, block.last_committed_batch);

        visit_or_error!(self, &block.block_program_states);

        visit_or_error!(self, &block.block_schedule);

        visit_or_error!(self, &block.block_outcome);
    }

    fn walk_block_commitment_queue(&mut self, _block: H256, queue: &VecDeque<H256>) {
        for &block in queue {
            self.visit_block(block);
        }
    }

    fn walk_block_codes_queue(&mut self, _block: H256, queue: &VecDeque<CodeId>) {
        for &code in queue {
            self.visit_code_id(code);
        }
    }

    fn walk_program_id(&mut self, program_id: ActorId) {
        let Some(code_id) = self.db().program_code_id(program_id) else {
            self.on_db_error(DatabaseVisitorError::NoProgramCodeId(program_id));
            return;
        };

        self.visit_code_id(code_id);
    }

    fn walk_code_id(&mut self, code: CodeId) {
        visit_or_error!(self, code.code_valid);

        if let Some(original_code) = self.db().original_code(code) {
            self.visit_original_code(&original_code);
        } else {
            self.on_db_error(DatabaseVisitorError::NoOriginalCode(code));
        }

        if let Some(instrumented_code) = self
            .db()
            .instrumented_code(ethexe_runtime_common::RUNTIME_ID, code)
        {
            self.visit_instrumented_code(code, &instrumented_code);
        } else {
            self.on_db_error(DatabaseVisitorError::NoInstrumentedCode(code));
        }

        visit_or_error!(self, &code.code_metadata);
    }

    fn walk_block_program_states(&mut self, _block: H256, program_states: &ProgramStates) {
        for StateHashWithQueueSize {
            hash: program_state,
            cached_queue_size: _,
        } in program_states.values().copied()
        {
            visit_or_error!(self, program_state.as_ref());
        }
    }

    fn walk_program_state(&mut self, state: &ProgramState) {
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
                visit_or_error!(self, allocations.as_ref());
            }

            if let Some(memory_pages) = pages_hash.to_inner() {
                visit_or_error!(self, memory_pages.as_ref());
            }
        }

        self.visit_message_queue_hash_with_size(*queue);

        if let Some(waitlist) = waitlist_hash.to_inner() {
            visit_or_error!(self, waitlist.as_ref());
        }

        if let Some(dispatch_stash) = stash_hash.to_inner() {
            visit_or_error!(self, dispatch_stash.as_ref());
        }

        if let Some(mailbox) = mailbox_hash.to_inner() {
            visit_or_error!(self, mailbox.as_ref());
        }
    }

    fn walk_block_schedule(&mut self, block: H256, schedule: &Schedule) {
        for (&height, tasks) in schedule {
            self.visit_block_schedule_tasks(block, height, tasks);
        }
    }

    fn walk_block_schedule_tasks(&mut self, tasks: &BTreeSet<ScheduledTask>) {
        for task in tasks {
            self.visit_scheduled_task(task);
        }
    }

    fn walk_scheduled_task(&mut self, task: &ScheduledTask) {
        match *task {
            ScheduledTask::PauseProgram(program_id) => {
                self.visit_program_id(program_id);
            }
            ScheduledTask::RemoveCode(code_id) => {
                self.visit_code_id(code_id);
            }
            ScheduledTask::RemoveFromMailbox((program_id, _destination), _msg_id) => {
                self.visit_program_id(program_id);
            }
            ScheduledTask::RemoveFromWaitlist(program_id, _) => {
                self.visit_program_id(program_id);
            }
            ScheduledTask::RemovePausedProgram(program_id) => {
                self.visit_program_id(program_id);
            }
            ScheduledTask::WakeMessage(program_id, _) => {
                self.visit_program_id(program_id);
            }
            ScheduledTask::SendDispatch((program_id, _msg_id)) => {
                self.visit_program_id(program_id);
            }
            ScheduledTask::SendUserMessage {
                message_id: _,
                to_mailbox: program_id,
            } => {
                self.visit_program_id(program_id);
            }
            ScheduledTask::RemoveGasReservation(program_id, _) => {
                self.visit_program_id(program_id);
            }
            #[allow(deprecated)]
            ScheduledTask::RemoveResumeSession(_) => unreachable!("deprecated"),
        }
    }

    fn walk_block_outcome(&mut self, outcome: &BlockOutcome) {
        match outcome {
            BlockOutcome::Transitions(outcome) => {
                for transition in outcome {
                    self.visit_state_transition(transition);
                }
            }
            BlockOutcome::ForcedNonEmpty => {}
        }
    }

    fn walk_state_transition(&mut self, state_transition: &StateTransition) {
        let &StateTransition {
            actor_id,
            new_state_hash: program_state,
            exited: _,
            inheritor: _,
            value_to_receive: _,
            value_claims: _,
            messages: _,
        } = state_transition;

        self.visit_program_id(actor_id);

        if program_state != H256::zero() {
            visit_or_error!(self, program_state.as_ref());
        }
    }

    fn walk_memory_pages(&mut self, pages: &MemoryPages) {
        for memory_pages_region in pages.to_inner().into_iter().flat_map(MaybeHashOf::to_inner) {
            visit_or_error!(self, memory_pages_region.as_ref());
        }
    }

    fn walk_memory_pages_region(&mut self, region: &MemoryPagesRegion) {
        for &page_data in region.as_inner().values() {
            visit_or_error!(self, page_data.as_ref());
        }
    }

    fn walk_message_queue_hash_with_size(
        &mut self,
        message_queue_hash_with_size: MessageQueueHashWithSize,
    ) {
        if let Some(message_queue) = message_queue_hash_with_size.hash.to_inner() {
            visit_or_error!(self, message_queue.as_ref());
        }
    }

    fn walk_message_queue(&mut self, queue: &MessageQueue) {
        for dispatch in queue.as_ref() {
            self.visit_payload_lookup(&dispatch.payload);
        }
    }

    fn walk_waitlist(&mut self, waitlist: &Waitlist) {
        for Expiring {
            value: dispatch,
            expiry: _,
        } in waitlist.as_ref().values()
        {
            self.visit_payload_lookup(&dispatch.payload);
        }
    }

    fn walk_mailbox(&mut self, mailbox: &Mailbox) {
        for &user_mailbox in mailbox.as_ref().values() {
            visit_or_error!(self, user_mailbox.as_ref());
        }
    }

    fn walk_user_mailbox(&mut self, user_mailbox: &UserMailbox) {
        for Expiring {
            value: msg,
            expiry: _,
        } in user_mailbox.as_ref().values()
        {
            self.visit_payload_lookup(&msg.payload);
        }
    }

    fn walk_dispatch_stash(&mut self, stash: &DispatchStash) {
        for Expiring {
            value: (dispatch, _user_id),
            expiry: _,
        } in stash.as_ref().values()
        {
            self.visit_payload_lookup(&dispatch.payload);
        }
    }

    fn walk_payload_lookup(&mut self, payload_lookup: &PayloadLookup) {
        match payload_lookup {
            PayloadLookup::Direct(payload) => {
                self.visit_payload(payload);
            }
            PayloadLookup::Stored(payload) => {
                let payload = *payload;
                visit_or_error!(self, payload.as_ref());
            }
        }
    }
}

impl<V> DatabaseVisitor for DatabaseWalker<V>
where
    V: DatabaseVisitor,
{
    fn db(&self) -> &dyn DatabaseVisitorStorage {
        self.visitor.db()
    }

    fn on_db_error(&mut self, error: DatabaseVisitorError) {
        self.visitor.on_db_error(error);
    }

    fn visit_chain(&mut self, head: H256, bottom: H256) {
        self.visitor.visit_chain(head, bottom);
        self.walk_chain(head, bottom);
    }

    fn visit_block(&mut self, block: H256) {
        // avoid recursion
        if self.visited_blocks.insert(block) {
            self.visitor.visit_block(block);
            self.walk_block(block);
        }
    }

    fn visit_block_meta(&mut self, _block: H256, _meta: &BlockMeta) {
        self.visitor.visit_block_meta(_block, _meta);
    }

    fn visit_block_header(&mut self, _block: H256, _header: BlockHeader) {
        self.visitor.visit_block_header(_block, _header);
    }

    fn visit_block_events(&mut self, _block: H256, _events: &[BlockEvent]) {
        self.visitor.visit_block_events(_block, _events);
    }

    fn visit_block_commitment_queue(&mut self, _block: H256, _queue: &VecDeque<H256>) {
        self.visitor.visit_block_commitment_queue(_block, _queue);
        self.walk_block_commitment_queue(_block, _queue);
    }

    fn visit_block_codes_queue(&mut self, _block: H256, queue: &VecDeque<CodeId>) {
        self.visitor.visit_block_codes_queue(_block, queue);
        self.walk_block_codes_queue(_block, queue)
    }

    fn visit_code_id(&mut self, code_id: CodeId) {
        self.visitor.visit_code_id(code_id);
        self.walk_code_id(code_id);
    }

    fn visit_code_valid(&mut self, _code_id: CodeId, _code_valid: bool) {
        self.visitor.visit_code_valid(_code_id, _code_valid);
    }

    fn visit_original_code(&mut self, _original_code: &[u8]) {
        self.visitor.visit_original_code(_original_code);
    }

    fn visit_instrumented_code(&mut self, _code_id: CodeId, _instrumented_code: &InstrumentedCode) {
        self.visitor
            .visit_instrumented_code(_code_id, _instrumented_code);
    }

    fn visit_code_metadata(&mut self, _code_id: CodeId, _metadata: &CodeMetadata) {
        self.visitor.visit_code_metadata(_code_id, _metadata);
    }

    fn visit_program_id(&mut self, _program_id: ActorId) {
        self.visitor.visit_program_id(_program_id);
        self.walk_program_id(_program_id);
    }

    fn visit_previous_non_empty_block(&mut self, _block: H256, _previous_non_empty_block: H256) {
        self.visitor
            .visit_previous_non_empty_block(_block, _previous_non_empty_block);
    }

    fn visit_last_committed_batch(&mut self, _block: H256, _batch: Digest) {
        self.visitor.visit_last_committed_batch(_block, _batch);
    }

    fn visit_block_program_states(&mut self, _block: H256, program_states: &ProgramStates) {
        self.visitor
            .visit_block_program_states(_block, program_states);
        self.walk_block_program_states(_block, program_states)
    }

    fn visit_program_state(&mut self, state: &ProgramState) {
        self.visitor.visit_program_state(state);
        self.walk_program_state(state);
    }

    fn visit_block_schedule(&mut self, block: H256, schedule: &Schedule) {
        self.visitor.visit_block_schedule(block, schedule);
        self.walk_block_schedule(block, schedule);
    }

    fn visit_block_schedule_tasks(
        &mut self,
        _block: H256,
        _height: u32,
        tasks: &BTreeSet<ScheduledTask>,
    ) {
        self.visitor
            .visit_block_schedule_tasks(_block, _height, tasks);
        self.walk_block_schedule_tasks(tasks);
    }

    fn visit_scheduled_task(&mut self, task: &ScheduledTask) {
        self.visitor.visit_scheduled_task(task);
        self.walk_scheduled_task(task);
    }

    fn visit_block_outcome(&mut self, _block: H256, outcome: &BlockOutcome) {
        self.visitor.visit_block_outcome(_block, outcome);
        self.walk_block_outcome(outcome);
    }

    fn visit_state_transition(&mut self, state_transition: &StateTransition) {
        self.visitor.visit_state_transition(state_transition);
        self.walk_state_transition(state_transition);
    }

    fn visit_allocations(&mut self, _allocations: &Allocations) {
        self.visitor.visit_allocations(_allocations);
    }

    fn visit_memory_pages(&mut self, memory_pages: &MemoryPages) {
        self.visitor.visit_memory_pages(memory_pages);
        self.walk_memory_pages(memory_pages);
    }

    fn visit_memory_pages_region(&mut self, memory_pages_region: &MemoryPagesRegion) {
        self.visitor.visit_memory_pages_region(memory_pages_region);
        self.walk_memory_pages_region(memory_pages_region);
    }

    fn visit_page_data(&mut self, _page_data: &[u8]) {
        self.visitor.visit_page_data(_page_data);
    }

    fn visit_payload_lookup(&mut self, payload_lookup: &PayloadLookup) {
        self.visitor.visit_payload_lookup(payload_lookup);
        self.walk_payload_lookup(payload_lookup);
    }

    fn visit_payload(&mut self, _payload: &Payload) {
        self.visitor.visit_payload(_payload);
    }

    fn visit_message_queue_hash_with_size(
        &mut self,
        queue_hash_with_size: MessageQueueHashWithSize,
    ) {
        self.visitor
            .visit_message_queue_hash_with_size(queue_hash_with_size);
        self.walk_message_queue_hash_with_size(queue_hash_with_size);
    }

    fn visit_message_queue(&mut self, queue: &MessageQueue) {
        self.visitor.visit_message_queue(queue);
        self.walk_message_queue(queue);
    }

    fn visit_waitlist(&mut self, waitlist: &Waitlist) {
        self.visitor.visit_waitlist(waitlist);
        self.walk_waitlist(waitlist);
    }

    fn visit_mailbox(&mut self, mailbox: &Mailbox) {
        self.visitor.visit_mailbox(mailbox);
        self.walk_mailbox(mailbox);
    }

    fn visit_user_mailbox(&mut self, user_mailbox: &UserMailbox) {
        self.visitor.visit_user_mailbox(user_mailbox);
        self.walk_user_mailbox(user_mailbox);
    }

    fn visit_dispatch_stash(&mut self, stash: &DispatchStash) {
        self.visitor.visit_dispatch_stash(stash);
        self.walk_dispatch_stash(stash);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Database;
    use gprimitives::MessageId;
    use std::collections::BTreeMap;

    #[derive(Debug)]
    struct TestVisitor {
        db: Database,
        visited_code_ids: Vec<CodeId>,
        visited_program_ids: Vec<ActorId>,
        visited_payloads: Vec<Payload>,
        errors: Vec<DatabaseVisitorError>,
    }

    impl TestVisitor {
        fn new() -> Self {
            Self {
                db: Database::memory(),
                visited_code_ids: vec![],
                visited_program_ids: vec![],
                visited_payloads: vec![],
                errors: vec![],
            }
        }
    }

    impl DatabaseVisitor for TestVisitor {
        fn db(&self) -> &dyn DatabaseVisitorStorage {
            &self.db
        }

        fn on_db_error(&mut self, error: DatabaseVisitorError) {
            self.errors.push(error);
        }

        fn visit_code_id(&mut self, code_id: CodeId) {
            self.visited_code_ids.push(code_id);
        }

        fn visit_program_id(&mut self, program_id: ActorId) {
            self.visited_program_ids.push(program_id);
        }

        fn visit_payload(&mut self, payload: &Payload) {
            self.visited_payloads.push(payload.clone());
        }
    }

    #[test]
    fn walk_chain_basic() {
        let mut visitor = TestVisitor::new();
        let head = H256::from_low_u64_be(1);
        let bottom = H256::from_low_u64_be(2);

        // This will fail because we don't have the block header in the database
        DatabaseWalker::new(&mut visitor).visit_chain(head, bottom);

        // Should have attempted to visit the head block
        assert!(!visitor.errors.is_empty());
        assert!(
            visitor
                .errors
                .contains(&DatabaseVisitorError::NoBlockHeader(head))
        );
    }

    #[test]
    fn walk_block_with_missing_data() {
        let mut visitor = TestVisitor::new();
        let block_hash = H256::from_low_u64_be(42);

        DatabaseWalker::new(&mut visitor).visit_block(block_hash);

        // Should have errors for all missing block data
        let expected_errors = [
            DatabaseVisitorError::NoBlockHeader(block_hash),
            DatabaseVisitorError::NoBlockEvents(block_hash),
            DatabaseVisitorError::NoBlockCommitmentQueue(block_hash),
            DatabaseVisitorError::NoBlockCodesQueue(block_hash),
            DatabaseVisitorError::NoPreviousNonEmptyBlock(block_hash),
            DatabaseVisitorError::NoLastCommittedBatch(block_hash),
            DatabaseVisitorError::NoBlockProgramStates(block_hash),
            DatabaseVisitorError::NoBlockSchedule(block_hash),
            DatabaseVisitorError::NoBlockOutcome(block_hash),
        ];

        for expected_error in expected_errors {
            assert!(visitor.errors.contains(&expected_error));
        }
    }

    #[test]
    fn test_walk_block_codes_queue() {
        let mut visitor = TestVisitor::new();

        let block_hash = H256::random();
        let code_id1 = CodeId::from([1u8; 32]);
        let code_id2 = CodeId::from([2u8; 32]);
        let mut queue = VecDeque::new();
        queue.push_back(code_id1);
        queue.push_back(code_id2);

        DatabaseWalker::new(&mut visitor).visit_block_codes_queue(block_hash, &queue);

        assert_eq!(visitor.visited_code_ids.len(), 2);
        assert!(visitor.visited_code_ids.contains(&code_id1));
        assert!(visitor.visited_code_ids.contains(&code_id2));
    }

    #[test]
    fn test_walk_block_program_states() {
        let mut visitor = TestVisitor::new();

        let block_hash = H256::random();
        let program_id = ActorId::from([3u8; 32]);
        let state_hash = H256::random();

        let mut program_states = BTreeMap::new();
        program_states.insert(
            program_id,
            StateHashWithQueueSize {
                hash: state_hash,
                cached_queue_size: 0,
            },
        );

        DatabaseWalker::new(&mut visitor).visit_block_program_states(block_hash, &program_states);

        // Should have error because program state is not in database
        assert!(
            visitor
                .errors
                .contains(&DatabaseVisitorError::NoProgramState(state_hash))
        );
    }

    #[test]
    fn walk_program_id_missing_code() {
        let mut visitor = TestVisitor::new();
        let program_id = ActorId::from([5u8; 32]);

        DatabaseWalker::new(&mut visitor).visit_program_id(program_id);

        // Should have error because program code ID is not in database
        assert!(
            visitor
                .errors
                .contains(&DatabaseVisitorError::NoProgramCodeId(program_id))
        );
    }

    #[test]
    fn test_no_code_valid_error() {
        let code_id = CodeId::from(1);

        let mut visitor = TestVisitor::new();
        DatabaseWalker::new(&mut visitor).visit_code_id(code_id);

        let expected_errors = [
            DatabaseVisitorError::NoCodeValid(code_id),
            DatabaseVisitorError::NoOriginalCode(code_id),
            DatabaseVisitorError::NoInstrumentedCode(code_id),
            DatabaseVisitorError::NoCodeMetadata(code_id),
        ];

        for expected_error in expected_errors {
            assert!(visitor.errors.contains(&expected_error));
        }
    }

    #[test]
    fn walk_scheduled_task_pause_program() {
        let mut visitor = TestVisitor::new();
        let program_id = ActorId::from([6u8; 32]);
        let task = ScheduledTask::PauseProgram(program_id);

        DatabaseWalker::new(&mut visitor).visit_scheduled_task(&task);

        assert!(visitor.visited_program_ids.contains(&program_id));
    }

    #[test]
    fn walk_scheduled_task_remove_code() {
        let mut visitor = TestVisitor::new();
        let code_id = CodeId::from([7u8; 32]);
        let task = ScheduledTask::RemoveCode(code_id);

        DatabaseWalker::new(&mut visitor).visit_scheduled_task(&task);

        assert!(visitor.visited_code_ids.contains(&code_id));
    }

    #[test]
    fn walk_scheduled_task_wake_message() {
        let mut visitor = TestVisitor::new();
        let program_id = ActorId::from([8u8; 32]);
        let msg_id = MessageId::from([9u8; 32]);
        let task = ScheduledTask::WakeMessage(program_id, msg_id);

        DatabaseWalker::new(&mut visitor).visit_scheduled_task(&task);

        assert!(visitor.visited_program_ids.contains(&program_id));
    }

    #[test]
    fn test_walk_block_schedule_tasks() {
        let mut visitor = TestVisitor::new();

        let block_hash = H256::random();
        let program_id1 = ActorId::from([10u8; 32]);
        let program_id2 = ActorId::from([11u8; 32]);
        let code_id = CodeId::from([12u8; 32]);

        let mut tasks = BTreeSet::new();
        tasks.insert(ScheduledTask::PauseProgram(program_id1));
        tasks.insert(ScheduledTask::RemoveCode(code_id));
        tasks.insert(ScheduledTask::WakeMessage(program_id2, MessageId::zero()));

        DatabaseWalker::new(&mut visitor).visit_block_schedule_tasks(block_hash, 123, &tasks);

        assert!(visitor.visited_program_ids.contains(&program_id1));
        assert!(visitor.visited_program_ids.contains(&program_id2));
        assert!(visitor.visited_code_ids.contains(&code_id));
    }

    #[test]
    fn test_walk_block_schedule() {
        let mut visitor = TestVisitor::new();
        let block_hash = H256::from([13u8; 32]);
        let program_id = ActorId::from([14u8; 32]);

        let mut schedule = BTreeMap::new();
        let mut tasks = BTreeSet::new();
        tasks.insert(ScheduledTask::PauseProgram(program_id));
        schedule.insert(1000u32, tasks);

        DatabaseWalker::new(&mut visitor).visit_block_schedule(block_hash, &schedule);

        assert!(visitor.visited_program_ids.contains(&program_id));
    }

    #[test]
    fn walk_block_outcome() {
        let mut visitor = TestVisitor::new();
        let block_hash = H256::from([16u8; 32]);
        let actor_id = ActorId::from([15u8; 32]);
        let new_state_hash = H256::random();

        DatabaseWalker::new(&mut visitor).visit_block_outcome(
            block_hash,
            &BlockOutcome::Transitions(vec![StateTransition {
                actor_id,
                new_state_hash,
                exited: false,
                inheritor: Default::default(),
                value_to_receive: 0,
                value_claims: vec![],
                messages: vec![],
            }]),
        );

        assert!(
            visitor
                .errors
                .contains(&DatabaseVisitorError::NoProgramCodeId(actor_id))
        );
        assert!(
            visitor
                .errors
                .contains(&DatabaseVisitorError::NoProgramState(new_state_hash))
        );
    }

    #[test]
    fn test_walk_state_transition() {
        let mut visitor = TestVisitor::new();
        let actor_id = ActorId::from([17u8; 32]);
        let new_state_hash = H256::from([18u8; 32]);

        let state_transition = StateTransition {
            actor_id,
            new_state_hash,
            exited: false,
            inheritor: ActorId::zero(),
            value_to_receive: 0,
            value_claims: Vec::new(),
            messages: Vec::new(),
        };

        DatabaseWalker::new(&mut visitor).visit_state_transition(&state_transition);

        assert!(visitor.visited_program_ids.contains(&actor_id));
        // Should have error for missing program state
        assert!(
            visitor
                .errors
                .contains(&DatabaseVisitorError::NoProgramState(new_state_hash))
        );
    }

    #[test]
    fn walk_state_transition_zero_state_hash() {
        let mut visitor = TestVisitor::new();
        let actor_id = ActorId::from([19u8; 32]);

        let state_transition = StateTransition {
            actor_id,
            new_state_hash: H256::zero(),
            exited: false,
            inheritor: ActorId::zero(),
            value_to_receive: 0,
            value_claims: Vec::new(),
            messages: Vec::new(),
        };

        DatabaseWalker::new(&mut visitor).visit_state_transition(&state_transition);

        assert!(visitor.visited_program_ids.contains(&actor_id));
        // Should not try to get program state for zero hash
        assert!(!visitor.errors.iter().any(
            |e| matches!(e, DatabaseVisitorError::NoProgramState(hash) if *hash == H256::zero())
        ));
    }

    #[test]
    fn walk_payload_lookup_direct() {
        let mut visitor = TestVisitor::new();
        let payload_data = vec![1, 2, 3, 4];
        let payload = Payload::try_from(payload_data.clone()).unwrap();
        let payload_lookup = PayloadLookup::Direct(payload.clone());

        DatabaseWalker::new(&mut visitor).visit_payload_lookup(&payload_lookup);

        assert!(visitor.visited_payloads.contains(&payload));
    }

    #[test]
    fn walk_payload_lookup_stored() {
        let mut visitor = TestVisitor::new();
        let payload_hash = unsafe { HashOf::<Payload>::new(H256::zero()) };
        let payload_lookup = PayloadLookup::Stored(payload_hash);

        DatabaseWalker::new(&mut visitor).visit_payload_lookup(&payload_lookup);

        // Should have error for missing stored payload
        assert!(
            visitor
                .errors
                .contains(&DatabaseVisitorError::NoPayload(payload_hash))
        );
    }
}
