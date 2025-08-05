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

use crate::iterator::{DatabaseIterator, DatabaseIteratorError, DatabaseIteratorStorage, Node};
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
    memory::PageBuf,
};
use gprimitives::{ActorId, CodeId, H256};
use std::collections::{BTreeSet, VecDeque};

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
            fn visit_node(visitor: &mut impl DatabaseVisitor, node: Node) {
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
    DatabaseIterator::new(visitor.clone_boxed_db())
        .start(node.into())
        .for_each(|node| visit_node(visitor, node));
}
