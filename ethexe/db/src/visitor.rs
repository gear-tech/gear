// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use crate::iterator::{DatabaseIterator, DatabaseIteratorError, DatabaseIteratorStorage, Node};
use ethexe_common::{
    BlockHeader, ProgramStates, Schedule, ScheduledTask,
    db::{BlockMeta, CompactMb, MbMeta},
    events::BlockEvent,
    gear::StateTransition,
};
use ethexe_runtime_common::state::{
    Allocations, DispatchStash, Mailbox, MemoryPages, MemoryPagesRegion, MessageQueue,
    MessageQueueHashWithSize, PayloadLookup, ProgramState, UserMailbox, Waitlist,
};
use gear_core::{
    buffer::Payload,
    code::{CodeMetadata, InstrumentedCode},
    memory::PageBuf,
};
use gprimitives::{ActorId, CodeId, H256};
use std::collections::BTreeSet;

macro_rules! define_visitor {
    ($( $variant:ident($node:ident { $( $field:ident: $ty:ty, )* }) )*) => {
        paste::paste! {
            #[auto_impl::auto_impl(&mut, Box)]
            pub trait DatabaseVisitor: Sized {
                fn db(&self) -> &dyn DatabaseIteratorStorage;

                fn clone_boxed_db(&self) -> Box<dyn DatabaseIteratorStorage>;

                fn on_db_error(&mut self, error: DatabaseIteratorError);

                $(
                    #[allow(unused_variables)]
                    fn [< visit_ $variant:snake >] (&mut self, $( $field: $ty, )*) {}
                )*
            }
        }
    };
}

crate::iterator::for_each_node!(define_visitor);

macro_rules! define_visit_node {
    ($( $variant:ident($node:ident { $( $field:ident: $ty:ty, )* }) )*) => {
        paste::paste! {
            pub fn visit_node(visitor: &mut impl DatabaseVisitor, node: Node) {
                match node {
                    $(
                        Node::$variant(node) => {
                            $(
                                let $field = node.$field;
                            )*
                            visitor.[< visit_ $variant:snake >]($( $field, )*);
                        },
                    )*
                    Node::Error(error) => {
                        visitor.on_db_error(error);
                    }
                }
            }
        }
    };
}

crate::iterator::for_each_node!(define_visit_node);

pub fn walk(visitor: &mut impl DatabaseVisitor, node: impl Into<Node>) {
    DatabaseIterator::new(visitor.clone_boxed_db(), node.into())
        .for_each(|node| visit_node(visitor, node));
}
