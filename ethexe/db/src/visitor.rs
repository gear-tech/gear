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
    AllocationsNode, BlockCodesQueueNode, BlockEventsNode, BlockHeaderNode, BlockMetaNode,
    BlockNode, BlockOutcomeNode, BlockProgramStatesNode, BlockScheduleNode, BlockScheduleTasksNode,
    ChainNode, CodeIdNode, CodeMetadataNode, CodeValidNode, DatabaseIterator,
    DatabaseIteratorError, DatabaseIteratorStorage, DispatchStashNode, InstrumentedCodeNode,
    MailboxNode, MemoryPagesRegionNode, MessageQueueHashWithSizeNode, MessageQueueNode, Node,
    OriginalCodeNode, PageDataNode, PayloadLookupNode, PayloadNode, ProgramIdNode,
    ProgramStateNode, ScheduledTaskNode, StateTransitionNode, UserMailboxNode, WaitlistNode,
};
use ethexe_common::{
    BlockHeader, BlockMeta, ProgramStates, Schedule, ScheduledTask, db::BlockOutcome,
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

    fn visit_block_codes_queue(&mut self, _block: H256, _queue: &VecDeque<CodeId>) {}

    fn visit_code_id(&mut self, _code_id: CodeId) {}

    fn visit_code_valid(&mut self, _code_id: CodeId, _code_valid: bool) {}

    fn visit_original_code(&mut self, _original_code: &[u8]) {}

    fn visit_instrumented_code(&mut self, _code_id: CodeId, _instrumented_code: &InstrumentedCode) {
    }

    fn visit_code_metadata(&mut self, _code_id: CodeId, _metadata: &CodeMetadata) {}

    fn visit_program_id(&mut self, _program_id: ActorId) {}

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
