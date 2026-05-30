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
            /// Callback-based traversal over the database graph.
            ///
            /// Implementations receive one `visit_*` call per [`Node`] variant that the
            /// [`walk`] helper encounters.  All `visit_*` methods have a default no-op body,
            /// so only the variants of interest need to be overridden.
            pub trait DatabaseVisitor: Sized {
                /// Returns a reference to the underlying storage used during traversal.
                fn db(&self) -> &dyn DatabaseIteratorStorage;

                /// Returns a heap-allocated clone of the underlying storage.
                ///
                /// Required so [`walk`] can construct a new [`DatabaseIterator`] that owns its
                /// own storage handle without borrowing `self`.
                fn clone_boxed_db(&self) -> Box<dyn DatabaseIteratorStorage>;

                /// Called when the iterator encounters a missing or invalid database record.
                ///
                /// The default [`walk`] loop does not abort on errors; implementations must
                /// decide whether to log, accumulate, or propagate them.
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
            /// Dispatches a single [`Node`] to the corresponding `visit_*` method on `visitor`.
            ///
            /// Error nodes are routed to [`DatabaseVisitor::on_db_error`] instead.
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

/// Traverses the database graph starting from `node`, calling [`visit_node`] for every
/// reachable [`Node`] in breadth-first order.
///
/// Internally creates a [`DatabaseIterator`] seeded with a storage clone obtained from
/// `visitor.clone_boxed_db()`, so no external storage handle is required at the call site.
pub fn walk(visitor: &mut impl DatabaseVisitor, node: impl Into<Node>) {
    DatabaseIterator::new(visitor.clone_boxed_db(), node.into())
        .for_each(|node| visit_node(visitor, node));
}
