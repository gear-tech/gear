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

use crate::iterator::{
    AllocationsNode, BlockCodesQueueNode, BlockCommitmentQueueNode, BlockEventsNode,
    BlockHeaderNode, BlockMetaNode, BlockNode, BlockOutcomeNode, BlockProgramStatesNode,
    BlockScheduleNode, BlockScheduleTasksNode, ChainNode, CodeIdNode, CodeMetadataNode,
    CodeValidNode, DatabaseIterator, DatabaseIteratorError, DatabaseIteratorStorage,
    DispatchStashNode, InstrumentedCodeNode, LastCommittedBatchNode, MailboxNode,
    MemoryPagesRegionNode, MessageQueueHashWithSizeNode, MessageQueueNode, Node, OriginalCodeNode,
    PageDataNode, PayloadLookupNode, PayloadNode, PreviousNonEmptyBlockNode, ProgramIdNode,
    ProgramStateNode, ScheduledTaskNode, StateTransitionNode, UserMailboxNode, WaitlistNode,
};
use ethexe_common::{
    BlockHeader, BlockMeta, Digest, ProgramStates, Schedule, ScheduledTask, db::BlockOutcome,
    events::BlockEvent, gear::StateTransition,
};
use ethexe_runtime_common::state::{
    Allocations, DispatchStash, Mailbox, MemoryPages, MemoryPagesRegion, MessageQueue,
    MessageQueueHashWithSize, PayloadLookup, ProgramState, UserMailbox, Waitlist,
};
use gear_core::{
    buffer::Payload,
    code::{CodeMetadata, InstrumentedCode},
};
use gprimitives::{ActorId, CodeId, H256};
use std::collections::{BTreeSet, VecDeque};

#[auto_impl::auto_impl(&mut, Box)]
pub trait DatabaseVisitor: Sized {
    fn db(&self) -> &dyn DatabaseIteratorStorage;

    fn clone_boxed_db(&self) -> Box<dyn DatabaseIteratorStorage>;

    fn on_db_error(&mut self, error: DatabaseIteratorError);

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

fn visit_node(visitor: &mut impl DatabaseVisitor, node: Node) {
    match node {
        Node::Chain(ChainNode { head, bottom }) => {
            visitor.visit_chain(head, bottom);
        }
        Node::Block(BlockNode { block }) => {
            visitor.visit_block(block);
        }
        Node::BlockMeta(BlockMetaNode { block, meta }) => {
            visitor.visit_block_meta(block, &meta);
        }
        Node::BlockHeader(BlockHeaderNode {
            block,
            block_header,
        }) => {
            visitor.visit_block_header(block, block_header);
        }
        Node::BlockEvents(BlockEventsNode {
            block,
            block_events,
        }) => {
            visitor.visit_block_events(block, &block_events);
        }
        Node::BlockCommitmentQueue(BlockCommitmentQueueNode {
            block,
            block_commitment_queue,
        }) => {
            visitor.visit_block_commitment_queue(block, &block_commitment_queue);
        }
        Node::BlockCodesQueue(BlockCodesQueueNode {
            block,
            block_codes_queue,
        }) => {
            visitor.visit_block_codes_queue(block, &block_codes_queue);
        }
        Node::CodeId(CodeIdNode { code_id }) => {
            visitor.visit_code_id(code_id);
        }
        Node::CodeValid(CodeValidNode {
            code_id,
            code_valid,
        }) => {
            visitor.visit_code_valid(code_id, code_valid);
        }
        Node::OriginalCode(OriginalCodeNode { original_code }) => {
            visitor.visit_original_code(&original_code);
        }
        Node::InstrumentedCode(InstrumentedCodeNode {
            code_id,
            instrumented_code,
        }) => {
            visitor.visit_instrumented_code(code_id, &instrumented_code);
        }
        Node::CodeMetadata(CodeMetadataNode {
            code_id,
            code_metadata,
        }) => {
            visitor.visit_code_metadata(code_id, &code_metadata);
        }
        Node::ProgramId(ProgramIdNode { program_id }) => {
            visitor.visit_program_id(program_id);
        }
        Node::PreviousNonEmptyBlock(PreviousNonEmptyBlockNode {
            block,
            previous_non_empty_block,
        }) => {
            visitor.visit_previous_non_empty_block(block, previous_non_empty_block);
        }
        Node::LastCommittedBatch(LastCommittedBatchNode {
            block,
            last_committed_batch,
        }) => {
            visitor.visit_last_committed_batch(block, last_committed_batch);
        }
        Node::BlockProgramStates(BlockProgramStatesNode {
            block,
            block_program_states,
        }) => {
            visitor.visit_block_program_states(block, &block_program_states);
        }
        Node::ProgramState(ProgramStateNode { program_state }) => {
            visitor.visit_program_state(&program_state);
        }
        Node::BlockSchedule(BlockScheduleNode {
            block,
            block_schedule,
        }) => {
            visitor.visit_block_schedule(block, &block_schedule);
        }
        Node::BlockScheduleTAsks(BlockScheduleTasksNode {
            block,
            height,
            tasks,
        }) => {
            visitor.visit_block_schedule_tasks(block, height, &tasks);
        }
        Node::ScheduledTask(ScheduledTaskNode { task }) => {
            visitor.visit_scheduled_task(&task);
        }
        Node::BlockOutcome(BlockOutcomeNode {
            block,
            block_outcome,
        }) => {
            visitor.visit_block_outcome(block, &block_outcome);
        }
        Node::StateTransition(StateTransitionNode { state_transition }) => {
            visitor.visit_state_transition(&state_transition);
        }
        Node::Allocations(AllocationsNode { allocations }) => {
            visitor.visit_allocations(&allocations);
        }
        Node::MemoryPages(node) => {
            visitor.visit_memory_pages(&node.memory_pages);
        }
        Node::MemoryPagesRegion(MemoryPagesRegionNode {
            memory_pages_region,
        }) => {
            visitor.visit_memory_pages_region(&memory_pages_region);
        }
        Node::PageData(PageDataNode { page_data }) => {
            visitor.visit_page_data(&page_data);
        }
        Node::PayloadLookup(PayloadLookupNode { payload_lookup }) => {
            visitor.visit_payload_lookup(&payload_lookup);
        }
        Node::Payload(PayloadNode { payload }) => {
            visitor.visit_payload(&payload);
        }
        Node::MessageQueueHashWithSize(MessageQueueHashWithSizeNode {
            queue_hash_with_size,
        }) => {
            visitor.visit_message_queue_hash_with_size(queue_hash_with_size);
        }
        Node::MessageQueue(MessageQueueNode { message_queue }) => {
            visitor.visit_message_queue(&message_queue);
        }
        Node::Waitlist(WaitlistNode { waitlist }) => {
            visitor.visit_waitlist(&waitlist);
        }
        Node::Mailbox(MailboxNode { mailbox }) => {
            visitor.visit_mailbox(&mailbox);
        }
        Node::UserMailbox(UserMailboxNode { user_mailbox }) => {
            visitor.visit_user_mailbox(&user_mailbox);
        }
        Node::DispatchStash(DispatchStashNode { dispatch_stash }) => {
            visitor.visit_dispatch_stash(&dispatch_stash);
        }
        Node::Error(error) => {
            visitor.on_db_error(error);
        }
    }
}

pub fn walk(visitor: &mut impl DatabaseVisitor, node: impl Into<Node>) {
    DatabaseIterator::new(visitor.clone_boxed_db())
        .start(node.into())
        .for_each(|node| visit_node(visitor, node));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Database, iterator::DatabaseIteratorError};
    use ethexe_common::StateHashWithQueueSize;
    use ethexe_runtime_common::state::HashOf;
    use gprimitives::MessageId;
    use std::collections::BTreeMap;

    #[derive(Debug)]
    struct TestVisitor {
        db: Database,
        visited_code_ids: Vec<CodeId>,
        visited_program_ids: Vec<ActorId>,
        visited_payloads: Vec<Payload>,
        errors: Vec<DatabaseIteratorError>,
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
        fn db(&self) -> &dyn DatabaseIteratorStorage {
            &self.db
        }

        fn clone_boxed_db(&self) -> Box<dyn DatabaseIteratorStorage> {
            Box::new(self.db.clone())
        }

        fn on_db_error(&mut self, error: DatabaseIteratorError) {
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
        walk(&mut visitor, ChainNode { head, bottom });

        // Should have attempted to visit the head block
        assert!(!visitor.errors.is_empty());
        assert!(
            visitor
                .errors
                .contains(&DatabaseIteratorError::NoBlockHeader(head))
        );
    }

    #[test]
    fn walk_block_with_missing_data() {
        let mut visitor = TestVisitor::new();
        let block = H256::from_low_u64_be(42);

        walk(&mut visitor, BlockNode { block });

        // Should have errors for all missing block data
        let expected_errors = [
            DatabaseIteratorError::NoBlockHeader(block),
            DatabaseIteratorError::NoBlockEvents(block),
            DatabaseIteratorError::NoBlockCommitmentQueue(block),
            DatabaseIteratorError::NoBlockCodesQueue(block),
            DatabaseIteratorError::NoPreviousNonEmptyBlock(block),
            DatabaseIteratorError::NoLastCommittedBatch(block),
            DatabaseIteratorError::NoBlockProgramStates(block),
            DatabaseIteratorError::NoBlockSchedule(block),
            DatabaseIteratorError::NoBlockOutcome(block),
        ];

        for expected_error in expected_errors {
            assert!(visitor.errors.contains(&expected_error));
        }
    }

    #[test]
    fn test_walk_block_codes_queue() {
        let mut visitor = TestVisitor::new();

        let block = H256::random();
        let code_id1 = CodeId::from([1u8; 32]);
        let code_id2 = CodeId::from([2u8; 32]);
        let mut queue = VecDeque::new();
        queue.push_back(code_id1);
        queue.push_back(code_id2);

        walk(
            &mut visitor,
            BlockCodesQueueNode {
                block,
                block_codes_queue: queue,
            },
        );

        assert_eq!(visitor.visited_code_ids.len(), 2);
        assert!(visitor.visited_code_ids.contains(&code_id1));
        assert!(visitor.visited_code_ids.contains(&code_id2));
    }

    #[test]
    fn test_walk_block_program_states() {
        let mut visitor = TestVisitor::new();

        let block = H256::random();
        let program_id = ActorId::from([3u8; 32]);
        let state_hash = H256::random();

        let mut block_program_states = BTreeMap::new();
        block_program_states.insert(
            program_id,
            StateHashWithQueueSize {
                hash: state_hash,
                cached_queue_size: 0,
            },
        );

        walk(
            &mut visitor,
            BlockProgramStatesNode {
                block,
                block_program_states,
            },
        );

        // Should have error because program state is not in database
        assert!(
            visitor
                .errors
                .contains(&DatabaseIteratorError::NoProgramState(state_hash))
        );
    }

    #[test]
    fn walk_program_id_missing_code() {
        let mut visitor = TestVisitor::new();
        let program_id = ActorId::from([5u8; 32]);

        walk(&mut visitor, ProgramIdNode { program_id });

        // Should have error because program code ID is not in database
        assert!(
            visitor
                .errors
                .contains(&DatabaseIteratorError::NoProgramCodeId(program_id))
        );
    }

    #[test]
    fn test_no_code_valid_error() {
        let code_id = CodeId::from(1);

        let mut visitor = TestVisitor::new();
        walk(&mut visitor, CodeIdNode { code_id });

        let expected_errors = [
            DatabaseIteratorError::NoCodeValid(code_id),
            DatabaseIteratorError::NoOriginalCode(code_id),
            DatabaseIteratorError::NoInstrumentedCode(code_id),
            DatabaseIteratorError::NoCodeMetadata(code_id),
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

        walk(&mut visitor, ScheduledTaskNode { task });

        assert!(visitor.visited_program_ids.contains(&program_id));
    }

    #[test]
    fn walk_scheduled_task_remove_code() {
        let mut visitor = TestVisitor::new();
        let code_id = CodeId::from([7u8; 32]);
        let task = ScheduledTask::RemoveCode(code_id);

        walk(&mut visitor, ScheduledTaskNode { task });

        assert!(visitor.visited_code_ids.contains(&code_id));
    }

    #[test]
    fn walk_scheduled_task_wake_message() {
        let mut visitor = TestVisitor::new();
        let program_id = ActorId::from([8u8; 32]);
        let msg_id = MessageId::from([9u8; 32]);
        let task = ScheduledTask::WakeMessage(program_id, msg_id);

        walk(&mut visitor, ScheduledTaskNode { task });

        assert!(visitor.visited_program_ids.contains(&program_id));
    }

    #[test]
    fn test_walk_block_schedule_tasks() {
        let mut visitor = TestVisitor::new();

        let block = H256::random();
        let program_id1 = ActorId::from([10u8; 32]);
        let program_id2 = ActorId::from([11u8; 32]);
        let code_id = CodeId::from([12u8; 32]);

        let mut tasks = BTreeSet::new();
        tasks.insert(ScheduledTask::PauseProgram(program_id1));
        tasks.insert(ScheduledTask::RemoveCode(code_id));
        tasks.insert(ScheduledTask::WakeMessage(program_id2, MessageId::zero()));

        walk(
            &mut visitor,
            BlockScheduleTasksNode {
                block,
                height: 123,
                tasks,
            },
        );

        assert!(visitor.visited_program_ids.contains(&program_id1));
        assert!(visitor.visited_program_ids.contains(&program_id2));
        assert!(visitor.visited_code_ids.contains(&code_id));
    }

    #[test]
    fn test_walk_block_schedule() {
        let mut visitor = TestVisitor::new();
        let block = H256::from([13u8; 32]);
        let program_id = ActorId::from([14u8; 32]);

        let mut block_schedule = BTreeMap::new();
        let mut tasks = BTreeSet::new();
        tasks.insert(ScheduledTask::PauseProgram(program_id));
        block_schedule.insert(1000u32, tasks);

        walk(
            &mut visitor,
            BlockScheduleNode {
                block,
                block_schedule,
            },
        );

        assert!(visitor.visited_program_ids.contains(&program_id));
    }

    #[test]
    fn walk_block_outcome() {
        let mut visitor = TestVisitor::new();
        let block = H256::from([16u8; 32]);
        let actor_id = ActorId::from([15u8; 32]);
        let new_state_hash = H256::random();

        walk(
            &mut visitor,
            BlockOutcomeNode {
                block,
                block_outcome: BlockOutcome::Transitions(vec![StateTransition {
                    actor_id,
                    new_state_hash,
                    exited: false,
                    inheritor: Default::default(),
                    value_to_receive: 0,
                    value_claims: vec![],
                    messages: vec![],
                }]),
            },
        );

        assert!(
            visitor
                .errors
                .contains(&DatabaseIteratorError::NoProgramCodeId(actor_id))
        );
        assert!(
            visitor
                .errors
                .contains(&DatabaseIteratorError::NoProgramState(new_state_hash))
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

        walk(&mut visitor, StateTransitionNode { state_transition });

        assert!(visitor.visited_program_ids.contains(&actor_id));
        // Should have error for missing program state
        assert!(
            visitor
                .errors
                .contains(&DatabaseIteratorError::NoProgramState(new_state_hash))
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

        walk(&mut visitor, StateTransitionNode { state_transition });

        assert!(visitor.visited_program_ids.contains(&actor_id));
        // Should not try to get program state for zero hash
        assert!(!visitor.errors.iter().any(
            |e| matches!(e, DatabaseIteratorError::NoProgramState(hash) if *hash == H256::zero())
        ));
    }

    #[test]
    fn walk_payload_lookup_direct() {
        let mut visitor = TestVisitor::new();
        let payload_data = vec![1, 2, 3, 4];
        let payload = Payload::try_from(payload_data.clone()).unwrap();
        let payload_lookup = PayloadLookup::Direct(payload.clone());

        walk(&mut visitor, PayloadLookupNode { payload_lookup });

        assert!(visitor.visited_payloads.contains(&payload));
    }

    #[test]
    fn walk_payload_lookup_stored() {
        let mut visitor = TestVisitor::new();
        let payload_hash = unsafe { HashOf::<Payload>::new(H256::zero()) };
        let payload_lookup = PayloadLookup::Stored(payload_hash);

        walk(&mut visitor, PayloadLookupNode { payload_lookup });

        // Should have error for missing stored payload
        assert!(
            visitor
                .errors
                .contains(&DatabaseIteratorError::NoPayload(payload_hash))
        );
    }
}
