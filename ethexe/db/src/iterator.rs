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
    Announce, BlockHeader, HashOf, MaybeHashOf, ProgramStates, Schedule, ScheduledTask,
    StateHashWithQueueSize,
    db::{
        AnnounceMeta, AnnounceStorageRO, BlockMeta, BlockMetaStorageRO, CodesStorageRO,
        LatestDataStorageRO, OnChainStorageRO,
    },
    events::BlockEvent,
    gear::StateTransition,
};
use ethexe_runtime_common::state::{
    ActiveProgram, Allocations, DispatchStash, Expiring, Mailbox, MemoryPages, MemoryPagesRegion,
    MessageQueue, MessageQueueHashWithSize, PayloadLookup, Program, ProgramState, Storage,
    UserMailbox, Waitlist,
};
use gear_core::{
    buffer::Payload,
    code::{CodeMetadata, InstrumentedCode},
    memory::PageBuf,
};
use gprimitives::{ActorId, CodeId, H256};
use std::{
    collections::{BTreeSet, HashSet, VecDeque},
    hash::{DefaultHasher, Hash, Hasher},
};

pub trait DatabaseIteratorStorage:
    OnChainStorageRO
    + BlockMetaStorageRO
    + AnnounceStorageRO
    + CodesStorageRO
    + LatestDataStorageRO
    + Storage
{
}

impl<
    T: OnChainStorageRO
        + BlockMetaStorageRO
        + AnnounceStorageRO
        + CodesStorageRO
        + LatestDataStorageRO
        + Storage,
> DatabaseIteratorStorage for T
{
}

macro_rules! node {
    (
        $(#[$($meta:meta)*])?
        pub enum Node {
            Error(DatabaseIteratorError),
            $(
                $variant:ident $([ $wrap:ident $lt:tt $gt:tt ])? (
                    $(#[$($node_meta:meta)*])?
                    pub struct $node:ident {
                        $(
                            pub $field:ident: $ty:ty,
                        )*
                    }
                ),
            )*
        }
    ) => {
        $(#[$($meta)*])?
        pub enum Node {
            Error(DatabaseIteratorError),
            $(
                $variant($( $wrap $lt )? $node $( $gt )?),
            )*
        }

        impl Node {
            $(
                paste::paste! {
                    pub fn [< into_ $variant:snake >] (self) -> Option<$( $wrap $lt )? $node $( $gt )?> {
                        match self {
                            Node::$variant(node) => Some(node),
                            _ => None,
                        }
                    }

                    pub fn [< as_ $variant:snake >] (&self) -> Option<&$( $wrap $lt )? $node $( $gt )?> {
                        match self {
                            Node::$variant(node) => Some(node),
                            _ => None,
                        }
                    }
                }
            )*
        }

        $(
            $(#[$($node_meta)*])?
            pub struct $node {
                $(
                    pub $field: $ty,
                )*
            }
        )*

        #[macro_export]
        macro_rules! for_each_node {
            ($mac:ident) => {
                $mac! {
                    $( $variant($node { $( $field: $ty, )* }) )*
                }
            };
        }

        // import should be here because it is unresolved import otherwise
        pub use for_each_node;
    };
}

node! {
    #[derive(Debug, Clone, Eq, PartialEq, Hash, derive_more::From, derive_more::IsVariant)]
    pub enum Node {
        Error(DatabaseIteratorError),
        Chain(
            #[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
            pub struct ChainNode {
                pub head: H256,
                pub bottom: H256,
            }
        ),
        Block(
            #[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
            pub struct BlockNode {
                pub block: H256,
            }
        ),
        BlockMeta(
            #[derive(Debug, Clone, Eq, PartialEq, Hash)]
            pub struct BlockMetaNode {
                pub block: H256,
                pub meta: BlockMeta,
            }
        ),
        BlockHeader(
            #[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
            pub struct BlockHeaderNode {
                pub block: H256,
                pub block_header: BlockHeader,
            }
        ),
        BlockEvents(
            #[derive(Debug, Clone, Eq, PartialEq, Hash)]
            pub struct BlockEventsNode {
                pub block: H256,
                pub block_events: Vec<BlockEvent>,
            }
        ),
        BlockSynced(
            #[derive(Debug, Clone, Eq, PartialEq, Hash)]
            pub struct BlockSyncedNode {
                pub block: H256,
                pub block_synced: bool,
            }
        ),
        Announce(
            #[derive(Debug, Clone, Eq, PartialEq, Hash)]
            pub struct AnnounceNode {
                pub announce_hash: HashOf<Announce>,
                pub announce: Announce,
            }
        ),
        AnnounceMeta(
            #[derive(Debug, Clone, Eq, PartialEq, Hash)]
            pub struct AnnounceMetaNode {
                pub announce_hash: HashOf<Announce>,
                pub announce_meta: AnnounceMeta,
            }
        ),
        CodeId(
            #[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
            pub struct CodeIdNode {
                pub code_id: CodeId,
            }
        ),
        CodeValid(
            #[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
            pub struct CodeValidNode {
                pub code_id: CodeId,
                pub code_valid: bool,
            }
        ),
        OriginalCode(
            #[derive(Debug, Clone, Eq, PartialEq, Hash)]
            pub struct OriginalCodeNode {
                pub original_code: Vec<u8>,
            }
        ),
        InstrumentedCode(
            #[derive(Debug, Clone, Eq, PartialEq, Hash)]
            pub struct InstrumentedCodeNode {
                pub code_id: CodeId,
                pub instrumented_code: InstrumentedCode,
            }
        ),
        CodeMetadata(
            #[derive(Debug, Clone, Eq, PartialEq, Hash)]
            pub struct CodeMetadataNode {
                pub code_id: CodeId,
                pub code_metadata: CodeMetadata,
            }
        ),
        ProgramId(
            #[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
            pub struct ProgramIdNode {
                pub program_id: ActorId,
            }
        ),
        AnnounceProgramStates(
            #[derive(Debug, Clone, Eq, PartialEq, Hash)]
            pub struct AnnounceProgramStatesNode {
                pub announce_hash: HashOf<Announce>,
                pub announce_program_states: ProgramStates,
            }
        ),
        ProgramState(
            #[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
            pub struct ProgramStateNode {
                pub program_state: ProgramState,
            }
        ),
        AnnounceSchedule(
            #[derive(Debug, Clone, Eq, PartialEq, Hash)]
            pub struct AnnounceScheduleNode {
                pub announce_hash: HashOf<Announce>,
                pub announce_schedule: Schedule,
            }
        ),
        AnnounceScheduleTasks(
            #[derive(Debug, Clone, Eq, PartialEq, Hash)]
            pub struct AnnounceScheduleTasksNode {
                pub announce_hash: HashOf<Announce>,
                pub height: u32,
                pub tasks: BTreeSet<ScheduledTask>,
            }
        ),
        ScheduledTask(
            #[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
            pub struct ScheduledTaskNode {
                pub task: ScheduledTask,
            }
        ),
        AnnounceOutcome(
            #[derive(Debug, Clone, Eq, PartialEq, Hash)]
            pub struct AnnounceOutcomeNode {
                pub announce_hash: HashOf<Announce>,
                pub announce_outcome: Vec<StateTransition>,
            }
        ),
        StateTransition(
            #[derive(Debug, Clone, Eq, PartialEq, Hash)]
            pub struct StateTransitionNode {
                pub state_transition: StateTransition,
            }
        ),
        Allocations(
            #[derive(Debug, Clone, Eq, PartialEq, Hash)]
            pub struct AllocationsNode {
                pub allocations: Allocations,
            }
        ),
        MemoryPages[Box<>](
            #[derive(Debug, Clone, Eq, PartialEq, Hash)]
            pub struct MemoryPagesNode {
                pub memory_pages: MemoryPages,
            }
        ),
        MemoryPagesRegion(
            #[derive(Debug, Clone, Eq, PartialEq, Hash)]
            pub struct MemoryPagesRegionNode {
                pub memory_pages_region: MemoryPagesRegion,
            }
        ),
        PageData(
            #[derive(Debug, Clone, Eq, PartialEq, Hash)]
            pub struct PageDataNode {
                pub page_data: PageBuf,
            }
        ),
        PayloadLookup(
            #[derive(Debug, Clone, Eq, PartialEq, Hash)]
            pub struct PayloadLookupNode {
                pub payload_lookup: PayloadLookup,
            }
        ),
        Payload(
            #[derive(Debug, Clone, Eq, PartialEq, Hash)]
            pub struct PayloadNode {
                pub payload: Payload,
            }
        ),
        MessageQueueHashWithSize(
            #[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
            pub struct MessageQueueHashWithSizeNode {
                pub queue_hash_with_size: MessageQueueHashWithSize,
            }
        ),
        MessageQueue(
            #[derive(Debug, Clone, Eq, PartialEq, Hash)]
            pub struct MessageQueueNode {
                pub message_queue: MessageQueue,
            }
        ),
        Waitlist(
            #[derive(Debug, Clone, Eq, PartialEq, Hash)]
            pub struct WaitlistNode {
                pub waitlist: Waitlist,
            }
        ),
        Mailbox(
            #[derive(Debug, Clone, Eq, PartialEq, Hash)]
            pub struct MailboxNode {
                pub mailbox: Mailbox,
            }
        ),
        UserMailbox(
            #[derive(Debug, Clone, Eq, PartialEq, Hash)]
            pub struct UserMailboxNode {
                pub user_mailbox: UserMailbox,
            }
        ),
        DispatchStash(
            #[derive(Debug, Clone, Eq, PartialEq, Hash)]
            pub struct DispatchStashNode {
                pub dispatch_stash: DispatchStash,
            }
        ),
    }
}

impl Node {
    pub fn into_error(self) -> Option<DatabaseIteratorError> {
        match self {
            Node::Error(error) => Some(error),
            _ => None,
        }
    }
}

impl From<MemoryPagesNode> for Node {
    fn from(value: MemoryPagesNode) -> Self {
        Self::MemoryPages(Box::new(value))
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, derive_more::IsVariant)]
pub enum DatabaseIteratorError {
    /* block */
    NoBlockHeader(H256),
    NoBlockEvents(H256),
    NoBlockAnnounces(H256),
    NoBlockCodesQueue(H256),

    NoAnnounce(HashOf<Announce>),
    NoAnnounceSchedule(HashOf<Announce>),
    NoAnnounceOutcome(HashOf<Announce>),
    NoAnnounceProgramStates(HashOf<Announce>),

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

pub struct DatabaseIterator<S> {
    storage: S,
    stack: VecDeque<Node>,
    visited_nodes: HashSet<u64>,
}

macro_rules! try_push_node {
    (with_hash: $this:ident.$method:ident($hash:ident)) => {
        if let Some(x) = $this.storage.$method($hash) {
            paste::paste! {
                $this.push_node(Node:: [< $method:camel >] ( [< $method:camel Node >] { $hash: $hash, $method: x } ));
            }
        } else {
            paste::paste! {
                $this.push_node(DatabaseIteratorError:: [< No $method:camel >] ($hash));
            }
        }
    };
    (no_hash: $this:ident.$method:ident($hash:ident)) => {
        if let Some(x) = $this.storage.$method($hash) {
            paste::paste! {
                $this.push_node(Node:: [< $method:camel >] ( [< $method:camel Node >] { $method: x } ));
            }
        } else {
            paste::paste! {
                $this.push_node(DatabaseIteratorError:: [< No $method:camel >] ($hash));
            }
        }
    };
}

impl<S> DatabaseIterator<S>
where
    S: DatabaseIteratorStorage,
{
    pub fn new(storage: S, node: impl Into<Node>) -> Self {
        let mut this = Self {
            storage,
            stack: Default::default(),
            visited_nodes: HashSet::new(),
        };
        this.push_node(node);
        this
    }

    fn push_node(&mut self, node: impl Into<Node>) {
        self.stack.push_back(node.into());
    }

    fn iter_node(&mut self, node: &Node) {
        match node {
            Node::Chain(node) => self.iter_chain(*node),
            Node::Block(node) => self.iter_block(*node),
            Node::BlockMeta(node) => self.iter_block_meta(node),
            Node::BlockHeader(_) => {}
            Node::BlockEvents(_) => {}
            Node::CodeId(node) => self.iter_code_id(*node),
            Node::CodeValid(_) => {}
            Node::OriginalCode(_) => {}
            Node::InstrumentedCode(_) => {}
            Node::CodeMetadata(_) => {}
            Node::ProgramId(node) => self.iter_program_id(*node),
            Node::AnnounceProgramStates(node) => self.iter_announce_program_states(node),
            Node::ProgramState(node) => self.iter_program_state(*node),
            Node::AnnounceSchedule(node) => self.iter_announce_schedule(node),
            Node::AnnounceScheduleTasks(node) => self.iter_announce_schedule_tasks(node),
            Node::ScheduledTask(node) => self.iter_scheduled_task(*node),
            Node::AnnounceOutcome(node) => self.iter_announce_outcome(node),
            Node::StateTransition(node) => self.iter_state_transition(node),
            Node::Allocations(_) => {}
            Node::MemoryPages(node) => self.iter_memory_pages(node),
            Node::MemoryPagesRegion(node) => self.iter_memory_pages_region(node),
            Node::PageData(_) => {}
            Node::PayloadLookup(node) => self.iter_payload_lookup(node),
            Node::Payload(_) => {}
            Node::MessageQueueHashWithSize(node) => self.iter_message_queue_hash_with_size(*node),
            Node::MessageQueue(node) => self.iter_message_queue(node),
            Node::Waitlist(node) => self.iter_waitlist(node),
            Node::Mailbox(node) => self.iter_mailbox(node),
            Node::UserMailbox(node) => self.iter_user_mailbox(node),
            Node::DispatchStash(node) => self.iter_dispatch_stash(node),
            Node::Error(_) => {}
            Node::Announce(node) => self.iter_announce(node),
            Node::AnnounceMeta(_) => {}
            Node::BlockSynced(_) => {}
        }
    }

    fn iter_chain(&mut self, ChainNode { head, bottom }: ChainNode) {
        let mut block = head;
        loop {
            self.push_node(BlockNode { block });

            if block == bottom {
                break;
            }

            let header = self.storage.block_header(block);
            if let Some(header) = header {
                block = header.parent_hash;
            } else {
                self.push_node(DatabaseIteratorError::NoBlockHeader(block));
                break;
            }
        }
    }

    fn iter_block(&mut self, BlockNode { block }: BlockNode) {
        let meta = self.storage.block_meta(block);
        self.push_node(BlockMetaNode { block, meta });

        try_push_node!(with_hash: self.block_header(block));

        try_push_node!(with_hash: self.block_events(block));

        self.push_node(BlockSyncedNode {
            block,
            block_synced: self.storage.block_synced(block),
        });
    }

    fn iter_block_meta(&mut self, BlockMetaNode { block, meta }: &BlockMetaNode) {
        let BlockMeta {
            prepared: _,
            announces,
            codes_queue,
            last_committed_batch: _,
            last_committed_announce: _,
        } = meta;

        if let Some(announces) = announces {
            for &announce_hash in announces {
                try_push_node!(with_hash: self.announce(announce_hash));
            }
        } else {
            self.push_node(DatabaseIteratorError::NoBlockAnnounces(*block));
        }

        if let Some(codes_queue) = codes_queue {
            for &code_id in codes_queue {
                self.push_node(CodeIdNode { code_id });
            }
        } else {
            self.push_node(Node::Error(DatabaseIteratorError::NoBlockCodesQueue(
                *block,
            )));
        }
    }

    fn iter_announce(
        &mut self,
        AnnounceNode {
            announce_hash,
            announce: _,
        }: &AnnounceNode,
    ) {
        let announce_hash = *announce_hash;
        try_push_node!(with_hash: self.announce_schedule(announce_hash));
        try_push_node!(with_hash: self.announce_outcome(announce_hash));
        try_push_node!(with_hash: self.announce_program_states(announce_hash));

        self.push_node(AnnounceMetaNode {
            announce_hash,
            announce_meta: self.storage.announce_meta(announce_hash),
        });

        // TODO #4830: offchain transactions
    }

    fn iter_program_id(&mut self, ProgramIdNode { program_id }: ProgramIdNode) {
        if let Some(code_id) = self.storage.program_code_id(program_id) {
            self.push_node(CodeIdNode { code_id });
        } else {
            self.push_node(DatabaseIteratorError::NoProgramCodeId(program_id));
        }
    }

    fn iter_code_id(&mut self, CodeIdNode { code_id }: CodeIdNode) {
        try_push_node!(with_hash: self.code_valid(code_id));

        try_push_node!(no_hash: self.original_code(code_id));

        if let Some(instrumented_code) = self
            .storage
            .instrumented_code(ethexe_runtime_common::RUNTIME_ID, code_id)
        {
            self.push_node(InstrumentedCodeNode {
                code_id,
                instrumented_code,
            });
        } else {
            self.push_node(DatabaseIteratorError::NoInstrumentedCode(code_id));
        }

        try_push_node!(with_hash: self.code_metadata(code_id));
    }

    fn iter_announce_program_states(
        &mut self,
        AnnounceProgramStatesNode {
            announce_hash: _,
            announce_program_states,
        }: &AnnounceProgramStatesNode,
    ) {
        for StateHashWithQueueSize {
            hash: program_state,
            canonical_queue_size: _,
            injected_queue_size: _,
        } in announce_program_states.values().copied()
        {
            try_push_node!(no_hash: self.program_state(program_state));
        }
    }

    fn iter_program_state(&mut self, ProgramStateNode { program_state }: ProgramStateNode) {
        let ProgramState {
            program,
            canonical_queue,
            injected_queue,
            waitlist_hash,
            stash_hash,
            mailbox_hash,
            balance: _,
            executable_balance: _,
        } = program_state;

        if let Program::Active(ActiveProgram {
            allocations_hash,
            pages_hash,
            memory_infix: _,
            initialized: _,
        }) = program
        {
            if let Some(allocations) = allocations_hash.to_inner() {
                try_push_node!(no_hash: self.allocations(allocations));
            }

            if let Some(memory_pages) = pages_hash.to_inner() {
                if let Some(x) = self.storage.memory_pages(memory_pages) {
                    self.push_node(Node::MemoryPages(Box::new(MemoryPagesNode {
                        memory_pages: x,
                    })));
                } else {
                    self.push_node(DatabaseIteratorError::NoMemoryPages(memory_pages));
                }
            }
        }

        self.push_node(MessageQueueHashWithSizeNode {
            queue_hash_with_size: canonical_queue,
        });

        self.push_node(MessageQueueHashWithSizeNode {
            queue_hash_with_size: injected_queue,
        });

        if let Some(waitlist) = waitlist_hash.to_inner() {
            try_push_node!(no_hash: self.waitlist(waitlist));
        }

        if let Some(dispatch_stash) = stash_hash.to_inner() {
            try_push_node!(no_hash: self.dispatch_stash(dispatch_stash));
        }

        if let Some(mailbox) = mailbox_hash.to_inner() {
            try_push_node!(no_hash: self.mailbox(mailbox));
        }
    }

    fn iter_announce_schedule(
        &mut self,
        AnnounceScheduleNode {
            announce_hash,
            announce_schedule,
        }: &AnnounceScheduleNode,
    ) {
        for (&height, tasks) in announce_schedule {
            self.push_node(AnnounceScheduleTasksNode {
                announce_hash: *announce_hash,
                height,
                tasks: tasks.clone(),
            });
        }
    }

    fn iter_announce_schedule_tasks(
        &mut self,
        AnnounceScheduleTasksNode {
            announce_hash: _,
            height: _,
            tasks,
        }: &AnnounceScheduleTasksNode,
    ) {
        for &task in tasks {
            self.push_node(ScheduledTaskNode { task });
        }
    }

    fn iter_scheduled_task(&mut self, ScheduledTaskNode { task }: ScheduledTaskNode) {
        match task {
            ScheduledTask::RemoveFromMailbox((program_id, _), _)
            | ScheduledTask::RemoveFromWaitlist(program_id, _)
            | ScheduledTask::WakeMessage(program_id, _)
            | ScheduledTask::SendDispatch((program_id, _))
            | ScheduledTask::SendUserMessage {
                message_id: _,
                to_mailbox: program_id,
            }
            | ScheduledTask::RemoveGasReservation(program_id, _) => {
                self.push_node(ProgramIdNode { program_id });
            }
        }
    }

    fn iter_announce_outcome(
        &mut self,
        AnnounceOutcomeNode {
            announce_hash: _,
            announce_outcome,
        }: &AnnounceOutcomeNode,
    ) {
        for state_transition in announce_outcome {
            self.push_node(StateTransitionNode {
                state_transition: state_transition.clone(),
            });
        }
    }

    fn iter_state_transition(
        &mut self,
        StateTransitionNode { state_transition }: &StateTransitionNode,
    ) {
        let StateTransition {
            actor_id,
            new_state_hash,
            exited: _,
            inheritor: _,
            value_to_receive: _,
            value_claims: _,
            messages: _,
        } = state_transition;

        let new_state_hash = *new_state_hash;

        self.push_node(ProgramIdNode {
            program_id: *actor_id,
        });

        if new_state_hash != H256::zero() {
            try_push_node!(no_hash: self.program_state(new_state_hash));
        }
    }

    fn iter_memory_pages(&mut self, MemoryPagesNode { memory_pages }: &MemoryPagesNode) {
        for region_hash in memory_pages
            .to_inner()
            .into_iter()
            .flat_map(MaybeHashOf::to_inner)
        {
            try_push_node!(no_hash: self.memory_pages_region(region_hash));
        }
    }

    fn iter_memory_pages_region(
        &mut self,
        MemoryPagesRegionNode {
            memory_pages_region,
        }: &MemoryPagesRegionNode,
    ) {
        for &page_data_hash in memory_pages_region.as_inner().values() {
            try_push_node!(no_hash: self.page_data(page_data_hash));
        }
    }

    fn iter_message_queue_hash_with_size(
        &mut self,
        MessageQueueHashWithSizeNode {
            queue_hash_with_size,
        }: MessageQueueHashWithSizeNode,
    ) {
        if let Some(message_queue_hash) = queue_hash_with_size.hash.to_inner() {
            try_push_node!(no_hash: self.message_queue(message_queue_hash));
        }
    }

    fn iter_message_queue(&mut self, MessageQueueNode { message_queue }: &MessageQueueNode) {
        for dispatch in message_queue.as_ref() {
            self.push_node(PayloadLookupNode {
                payload_lookup: dispatch.payload.clone(),
            });
        }
    }

    fn iter_waitlist(&mut self, WaitlistNode { waitlist }: &WaitlistNode) {
        for Expiring {
            value: dispatch,
            expiry: _,
        } in waitlist.as_ref().values()
        {
            self.push_node(PayloadLookupNode {
                payload_lookup: dispatch.payload.clone(),
            });
        }
    }

    fn iter_mailbox(&mut self, MailboxNode { mailbox }: &MailboxNode) {
        for &user_mailbox_hash in mailbox.as_ref().values() {
            try_push_node!(no_hash: self.user_mailbox(user_mailbox_hash));
        }
    }

    fn iter_user_mailbox(&mut self, UserMailboxNode { user_mailbox }: &UserMailboxNode) {
        for Expiring {
            value: msg,
            expiry: _,
        } in user_mailbox.as_ref().values()
        {
            self.push_node(PayloadLookupNode {
                payload_lookup: msg.payload.clone(),
            });
        }
    }

    fn iter_dispatch_stash(&mut self, DispatchStashNode { dispatch_stash }: &DispatchStashNode) {
        for Expiring {
            value: (dispatch, _user_id),
            expiry: _,
        } in dispatch_stash.as_ref().values()
        {
            self.push_node(PayloadLookupNode {
                payload_lookup: dispatch.payload.clone(),
            });
        }
    }

    fn iter_payload_lookup(&mut self, PayloadLookupNode { payload_lookup }: &PayloadLookupNode) {
        match payload_lookup {
            PayloadLookup::Direct(payload) => {
                self.push_node(PayloadNode {
                    payload: payload.clone(),
                });
            }
            PayloadLookup::Stored(payload_hash) => {
                let payload_hash = *payload_hash;
                try_push_node!(no_hash: self.payload(payload_hash));
            }
        }
    }
}

impl<S> Iterator for DatabaseIterator<S>
where
    S: DatabaseIteratorStorage,
{
    type Item = Node;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(node) = self.stack.pop_front() {
            let node_hash = {
                let mut hasher = DefaultHasher::new();
                node.hash(&mut hasher);
                hasher.finish()
            };

            if !self.visited_nodes.insert(node_hash) {
                // avoid recursion and duplicates
                continue;
            }

            self.iter_node(&node);

            return Some(node);
        }

        None
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::{Database, iterator::DatabaseIteratorError};
    use ethexe_common::StateHashWithQueueSize;
    use gprimitives::MessageId;
    use std::collections::BTreeMap;

    pub fn setup_db() -> Database {
        Database::memory()
    }

    #[test]
    fn walk_chain_basic() {
        let head = H256::from_low_u64_be(1);
        let bottom = H256::from_low_u64_be(2);

        // This will fail because we don't have the block header in the database
        assert!(
            DatabaseIterator::new(setup_db(), ChainNode { head, bottom })
                .filter_map(Node::into_error)
                .any(|error| error.is_no_block_header())
        );
    }

    #[test]
    fn walk_block_with_missing_data() {
        let block = H256::from_low_u64_be(42);

        let errors: Vec<_> = DatabaseIterator::new(setup_db(), BlockNode { block })
            .filter_map(Node::into_error)
            .collect();

        // Should have errors for all missing block data
        let expected_errors = [
            DatabaseIteratorError::NoBlockHeader(block),
            DatabaseIteratorError::NoBlockEvents(block),
            DatabaseIteratorError::NoBlockCodesQueue(block),
            DatabaseIteratorError::NoBlockAnnounces(block),
        ];

        for expected_error in expected_errors {
            assert!(
                errors.contains(&expected_error),
                "No expected error: {expected_error:?}",
            );
        }
    }

    #[test]
    fn walk_announce_program_states() {
        let announce_hash = HashOf::random();
        let program_id = ActorId::from([3u8; 32]);
        let state_hash = H256::random();

        let mut announce_program_states = BTreeMap::new();
        announce_program_states.insert(
            program_id,
            StateHashWithQueueSize {
                hash: state_hash,
                canonical_queue_size: 0,
                injected_queue_size: 0,
            },
        );

        let errors: Vec<_> = DatabaseIterator::new(
            setup_db(),
            AnnounceProgramStatesNode {
                announce_hash,
                announce_program_states,
            },
        )
        .filter_map(Node::into_error)
        .collect();

        assert!(errors.contains(&DatabaseIteratorError::NoProgramState(state_hash)));
    }

    #[test]
    fn walk_program_id_missing_code() {
        let program_id = ActorId::from([5u8; 32]);

        let errors: Vec<_> = DatabaseIterator::new(setup_db(), ProgramIdNode { program_id })
            .filter_map(Node::into_error)
            .collect();

        assert!(errors.contains(&DatabaseIteratorError::NoProgramCodeId(program_id)));
    }

    #[test]
    fn walk_code_id_missing_data() {
        let code_id = CodeId::from(1);

        let errors: Vec<_> = DatabaseIterator::new(setup_db(), CodeIdNode { code_id })
            .filter_map(Node::into_error)
            .collect();

        let expected_errors = [
            DatabaseIteratorError::NoCodeValid(code_id),
            DatabaseIteratorError::NoOriginalCode(code_id),
            DatabaseIteratorError::NoInstrumentedCode(code_id),
            DatabaseIteratorError::NoCodeMetadata(code_id),
        ];

        for expected_error in expected_errors {
            assert!(errors.contains(&expected_error));
        }
    }

    #[test]
    fn walk_block_schedule_tasks() {
        let announce_hash = HashOf::random();
        let program_id = ActorId::from([10u8; 32]);

        let mut tasks = BTreeSet::new();
        tasks.insert(ScheduledTask::WakeMessage(program_id, MessageId::zero()));

        let visited: Vec<_> = DatabaseIterator::new(
            setup_db(),
            AnnounceScheduleTasksNode {
                announce_hash,
                height: 123,
                tasks,
            },
        )
        .collect();

        let visited_programs: Vec<ActorId> = visited
            .iter()
            .cloned()
            .filter_map(Node::into_program_id)
            .map(|node| node.program_id)
            .collect();

        assert!(visited_programs.contains(&program_id));
    }

    #[test]
    fn walk_announce_schedule() {
        let announce_hash = HashOf::random();
        let program_id = ActorId::from([14u8; 32]);

        let mut announce_schedule = BTreeMap::new();
        let mut tasks = BTreeSet::new();
        tasks.insert(ScheduledTask::WakeMessage(program_id, MessageId::zero()));
        announce_schedule.insert(1000u32, tasks);

        let visited_programs: Vec<_> = DatabaseIterator::new(
            setup_db(),
            AnnounceScheduleNode {
                announce_hash,
                announce_schedule,
            },
        )
        .filter_map(Node::into_program_id)
        .map(|node| node.program_id)
        .collect();

        assert!(visited_programs.contains(&program_id));
    }

    #[test]
    fn walk_announce_outcome() {
        let announce_hash = HashOf::random();
        let actor_id = ActorId::from([15u8; 32]);
        let new_state_hash = H256::random();

        let errors: Vec<_> = DatabaseIterator::new(
            setup_db(),
            AnnounceOutcomeNode {
                announce_hash,
                announce_outcome: vec![StateTransition {
                    actor_id,
                    new_state_hash,
                    exited: false,
                    inheritor: Default::default(),
                    value_to_receive: 0,
                    value_claims: vec![],
                    messages: vec![],
                }],
            },
        )
        .filter_map(Node::into_error)
        .collect();

        assert!(errors.contains(&DatabaseIteratorError::NoProgramCodeId(actor_id)));
        assert!(errors.contains(&DatabaseIteratorError::NoProgramState(new_state_hash)));
    }

    #[test]
    fn walk_state_transition() {
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

        let nodes: Vec<_> =
            DatabaseIterator::new(setup_db(), StateTransitionNode { state_transition }).collect();

        let visited_programs: Vec<_> = nodes
            .iter()
            .cloned()
            .filter_map(Node::into_program_id)
            .map(|node| node.program_id)
            .collect();

        let errors: Vec<_> = nodes.into_iter().filter_map(Node::into_error).collect();

        assert!(visited_programs.contains(&actor_id));
        assert!(errors.contains(&DatabaseIteratorError::NoProgramState(new_state_hash)));
    }

    #[test]
    fn walk_state_transition_zero_state_hash() {
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

        let visited_states: Vec<_> =
            DatabaseIterator::new(setup_db(), StateTransitionNode { state_transition })
                .filter_map(Node::into_program_state)
                .map(|node| node.program_state)
                .collect();
        assert_eq!(visited_states, []);
    }

    #[test]
    fn walk_payload_lookup_direct() {
        let payload_data = vec![1, 2, 3, 4];
        let payload = Payload::try_from(payload_data.clone()).unwrap();
        let payload_lookup = PayloadLookup::Direct(payload.clone());

        let visited_payloads: Vec<_> =
            DatabaseIterator::new(setup_db(), PayloadLookupNode { payload_lookup })
                .filter_map(Node::into_payload)
                .map(|node| node.payload)
                .collect();

        assert!(visited_payloads.contains(&payload));
    }

    #[test]
    fn walk_payload_lookup_stored() {
        let db = setup_db();
        let payload = Payload::repeat(0xfe);
        let payload_hash = db.write_payload(payload.clone());

        let visited_payloads: Vec<_> = DatabaseIterator::new(
            db,
            PayloadLookupNode {
                payload_lookup: PayloadLookup::Stored(payload_hash),
            },
        )
        .filter_map(Node::into_payload)
        .map(|node| node.payload)
        .collect();

        assert_eq!(visited_payloads, [payload]);
    }
}
