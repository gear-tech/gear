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
    MemoryPagesRegion, MessageQueue, PayloadLookup, Program, ProgramState, Storage, UserMailbox,
    Waitlist,
};
use gear_core::{buffer::Payload, memory::PageBuf};
use gprimitives::{ActorId, CodeId, H256};
use std::collections::{BTreeSet, VecDeque};

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

    fn visit_block_outcome(&mut self, _block: H256, outcome: &BlockOutcome) {
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
    loop {
        visitor.visit_block(block);

        if block == bottom {
            break;
        }

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

    visit_or_error!(visitor, &block.block_outcome);
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

pub fn walk_block_outcome(visitor: &mut impl DatabaseVisitor, outcome: &BlockOutcome) {
    match outcome {
        BlockOutcome::Transitions(outcome) => {
            for transition in outcome {
                visitor.visit_state_transition(transition);
            }
        }
        BlockOutcome::ForcedNonEmpty => {}
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Database;
    use gprimitives::MessageId;
    use std::collections::BTreeMap;

    // Test visitor implementation to track visits and errors
    #[derive(Debug)]
    struct TestVisitor {
        db: Database,
        visited_blocks: Vec<H256>,
        visited_code_ids: Vec<CodeId>,
        visited_program_ids: Vec<ActorId>,
        visited_program_states: Vec<ProgramState>,
        visited_memory_pages: Vec<MemoryPages>,
        visited_payloads: Vec<Payload>,
        errors: Vec<DatabaseVisitorError>,
    }

    impl TestVisitor {
        fn new() -> Self {
            Self {
                db: Database::memory(),
                visited_blocks: vec![],
                visited_code_ids: vec![],
                visited_program_ids: vec![],
                visited_program_states: vec![],
                visited_memory_pages: vec![],
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

        fn visit_block(&mut self, block: H256) {
            self.visited_blocks.push(block);
            walk_block(self, block);
        }

        fn visit_code_id(&mut self, code_id: CodeId) {
            self.visited_code_ids.push(code_id);
        }

        fn visit_program_id(&mut self, program_id: ActorId) {
            self.visited_program_ids.push(program_id);
            walk_program_id(self, program_id);
        }

        fn visit_program_state(&mut self, state: &ProgramState) {
            self.visited_program_states.push(state.clone());
            walk_program_state(self, state);
        }

        fn visit_memory_pages(&mut self, memory_pages: &MemoryPages) {
            self.visited_memory_pages.push(memory_pages.clone());
            walk_memory_pages(self, memory_pages);
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
        visitor.visit_chain(head, bottom);

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

        visitor.visit_block(block_hash);

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
            assert!(
                visitor.errors.contains(&expected_error),
                "Expected error {:?} not found. Actual errors: {:?}",
                expected_error,
                visitor.errors
            );
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

        visitor.visit_block_codes_queue(block_hash, &queue);

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

        visitor.visit_block_program_states(block_hash, &program_states);

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

        visitor.visit_program_id(program_id);

        // Should have error because program code ID is not in database
        assert!(
            visitor
                .errors
                .contains(&DatabaseVisitorError::NoProgramCodeId(program_id))
        );
    }

    #[test]
    fn walk_scheduled_task_pause_program() {
        let mut visitor = TestVisitor::new();
        let program_id = ActorId::from([6u8; 32]);
        let task = ScheduledTask::PauseProgram(program_id);

        visitor.visit_scheduled_task(&task);

        assert!(visitor.visited_program_ids.contains(&program_id));
    }

    #[test]
    fn walk_scheduled_task_remove_code() {
        let mut visitor = TestVisitor::new();
        let code_id = CodeId::from([7u8; 32]);
        let task = ScheduledTask::RemoveCode(code_id);

        visitor.visit_scheduled_task(&task);

        assert!(visitor.visited_code_ids.contains(&code_id));
    }

    #[test]
    fn walk_scheduled_task_wake_message() {
        let mut visitor = TestVisitor::new();
        let program_id = ActorId::from([8u8; 32]);
        let msg_id = MessageId::from([9u8; 32]);
        let task = ScheduledTask::WakeMessage(program_id, msg_id);

        visitor.visit_scheduled_task(&task);

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

        visitor.visit_block_schedule_tasks(block_hash, 123, &tasks);

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

        visitor.visit_block_schedule(block_hash, &schedule);

        assert!(visitor.visited_program_ids.contains(&program_id));
    }

    #[test]
    fn walk_block_outcome() {
        let mut visitor = TestVisitor::new();
        let block_hash = H256::from([16u8; 32]);
        let actor_id = ActorId::from([15u8; 32]);
        let new_state_hash = H256::random();

        visitor.visit_block_outcome(
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

        visitor.visit_state_transition(&state_transition);

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

        visitor.visit_state_transition(&state_transition);

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

        visitor.visit_payload_lookup(&payload_lookup);

        assert!(visitor.visited_payloads.contains(&payload));
    }

    #[test]
    fn walk_payload_lookup_stored() {
        let mut visitor = TestVisitor::new();
        let payload_hash = unsafe { HashOf::<Payload>::new(H256::zero()) };
        let payload_lookup = PayloadLookup::Stored(payload_hash);

        visitor.visit_payload_lookup(&payload_lookup);

        // Should have error for missing stored payload
        assert!(
            visitor
                .errors
                .contains(&DatabaseVisitorError::NoPayload(payload_hash))
        );
    }
}
