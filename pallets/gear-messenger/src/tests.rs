// This file is part of Gear.

// Copyright (C) 2021-2022 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Unit tests module.

use super::*;
use crate::mock::*;
use common::storage::*;
use gear_core::{
    ids::MessageId,
    message::{DispatchKind, StoredDispatch, StoredMessage},
};

type SentOf = <Pallet<Test> as Messenger>::Sent;
type DequeuedOf = <Pallet<Test> as Messenger>::Dequeued;
type QueueProcessingOf = <Pallet<Test> as Messenger>::QueueProcessing;
type QueueOf = <Pallet<Test> as Messenger>::Queue;

pub(crate) fn init_logger() {
    let _ = env_logger::Builder::from_default_env()
        .format_module_path(false)
        .format_level(true)
        .try_init();
}

#[test]
fn sent_impl_works() {
    init_logger();
    new_test_ext().execute_with(|| {
        // Initial state of the block.
        assert_eq!(SentOf::get(), 0);

        // Bottom overflow check.
        SentOf::decrease();

        assert_eq!(SentOf::get(), 0);

        // Increasing/decreasing check.
        SentOf::increase();
        assert_eq!(SentOf::get(), 1);

        SentOf::increase();
        assert_eq!(SentOf::get(), 2);

        SentOf::decrease();

        assert_eq!(SentOf::get(), 1);

        // Clear check.
        SentOf::reset();
        assert_eq!(SentOf::get(), 0);

        // Value updates for future blocks.
        SentOf::increase();

        assert_eq!(SentOf::get(), 1);

        run_to_block(2);

        assert_eq!(SentOf::get(), 0);
    });
}

// Identical to `sent_impl_works` test, due to the same trait impl,
// but works on other storage and wont be actually manually used.
//
// See tests with auto increasing that parameter on pushes and pops from queue.
#[test]
fn dequeued_impl_works_manually() {
    init_logger();
    new_test_ext().execute_with(|| {
        // Initial state of the block.
        assert_eq!(DequeuedOf::get(), 0);

        // Bottom overflow check.
        DequeuedOf::decrease();

        assert_eq!(DequeuedOf::get(), 0);

        // Increasing/decreasing check.
        DequeuedOf::increase();
        assert_eq!(DequeuedOf::get(), 1);

        DequeuedOf::increase();
        assert_eq!(DequeuedOf::get(), 2);

        DequeuedOf::decrease();

        assert_eq!(DequeuedOf::get(), 1);

        // Clear check.
        DequeuedOf::reset();
        assert_eq!(DequeuedOf::get(), 0);

        // Value updates for future blocks.
        SentOf::increase();

        assert_eq!(SentOf::get(), 1);

        run_to_block(2);

        assert_eq!(DequeuedOf::get(), 0);
    });
}

// `QueueProcessing` won't be manually used.
//
// See tests with auto changing that parameter on pushes and pops from queue.
#[test]
fn queue_processing_impl_works_manually() {
    init_logger();
    new_test_ext().execute_with(|| {
        // Initial state of the block.
        assert!(QueueProcessingOf::allowed());

        // Invariants always.
        assert_ne!(QueueProcessingOf::allowed(), QueueProcessingOf::denied());

        // Denying check.
        QueueProcessingOf::deny();
        assert!(!QueueProcessingOf::allowed());
        assert_ne!(QueueProcessingOf::allowed(), QueueProcessingOf::denied());

        // Allowing check.
        QueueProcessingOf::allow();
        assert!(QueueProcessingOf::allowed());
        assert_ne!(QueueProcessingOf::allowed(), QueueProcessingOf::denied());

        // Value updates for future blocks.
        QueueProcessingOf::deny();
        assert!(QueueProcessingOf::denied());

        run_to_block(2);

        assert!(!QueueProcessingOf::denied());
    });
}

#[test]
fn queue_works() {
    init_logger();
    new_test_ext().execute_with(|| {
        // Initial state of the block.
        assert_eq!(DequeuedOf::get(), 0);
        assert_eq!(QueueOf::len(), 0);
        assert!(QueueProcessingOf::allowed());
        assert!(QueueOf::is_empty());

        // Dispatch constructor.
        let dispatch_with_id = |id: MessageId| {
            StoredDispatch::new(
                DispatchKind::Handle,
                StoredMessage::new(
                    id,
                    Default::default(),
                    Default::default(),
                    Default::default(),
                    Default::default(),
                    Default::default(),
                ),
                None,
            )
        };

        // Ids for future manipulations.
        let id_1: MessageId = 1.into();
        let id_2: MessageId = 2.into();
        let id_3: MessageId = 3.into();
        let id_4: MessageId = 4.into();
        let id_5: MessageId = 5.into();

        // Pop from empty queue.
        assert!(QueueOf::dequeue().expect("Algorithmic error").is_none());

        assert_eq!(DequeuedOf::get(), 0);
        assert_eq!(QueueOf::len(), 0);
        assert!(QueueOf::is_empty());

        // Push an element in empty queue.
        QueueOf::queue(dispatch_with_id(id_1)).expect("Algorithmic error");
        assert_eq!(DequeuedOf::get(), 0);
        assert_eq!(QueueOf::len(), 1);
        assert!(!QueueOf::is_empty());

        // Pop from single-element queue.
        assert_eq!(
            id_1,
            QueueOf::dequeue()
                .expect("Algorithmic error")
                .expect("No dispatches found")
                .id()
        );
        assert_eq!(DequeuedOf::get(), 1);
        assert_eq!(QueueOf::len(), 0);
        assert!(QueueOf::is_empty());

        // Push back many elements in empty queue.
        QueueOf::queue(dispatch_with_id(id_2)).expect("Algorithmic error");

        QueueOf::queue(dispatch_with_id(id_3)).expect("Algorithmic error");

        QueueOf::queue(dispatch_with_id(id_4)).expect("Algorithmic error");

        QueueOf::queue(dispatch_with_id(id_5)).expect("Algorithmic error");

        assert_eq!(DequeuedOf::get(), 1);
        assert_eq!(QueueOf::len(), 4);

        // Dequeued resets for future blocks.
        run_to_block(2);
        assert_eq!(DequeuedOf::get(), 0);
        assert_eq!(QueueOf::len(), 4);

        // Pop 2 of 4 messages.
        assert_eq!(
            id_2,
            QueueOf::dequeue()
                .expect("Algorithmic error")
                .expect("No dispatches found")
                .id()
        );

        let dispatch_3 = QueueOf::dequeue()
            .expect("Algorithmic error")
            .expect("No dispatches found");

        assert_eq!(id_3, dispatch_3.id());

        assert_eq!(DequeuedOf::get(), 2);
        assert_eq!(QueueOf::len(), 2);

        // Push front used only for requeueing element,
        // which was already in queue in current block,
        // because it decreased dequeued amount.
        assert!(QueueProcessingOf::allowed());

        QueueOf::requeue(dispatch_3).expect("Algorithmic error");

        assert!(QueueProcessingOf::denied());
        assert_eq!(DequeuedOf::get(), 1);
        assert_eq!(QueueOf::len(), 3);

        // Reset QueueProcessing deny.
        run_to_block(3);

        assert!(QueueProcessingOf::allowed());

        // Make the only one message be in queue.
        assert_eq!(
            id_3,
            QueueOf::dequeue()
                .expect("Algorithmic error")
                .expect("No dispatches found")
                .id()
        );

        assert_eq!(
            id_4,
            QueueOf::dequeue()
                .expect("Algorithmic error")
                .expect("No dispatches found")
                .id()
        );

        assert_eq!(QueueOf::len(), 1);

        // Reset dequeued amount.
        run_to_block(4);

        // Push front works on queue with one element
        let dispatch_5 = QueueOf::dequeue()
            .expect("Algorithmic error")
            .expect("No dispatches found");

        assert_eq!(id_5, dispatch_5.id());

        assert!(QueueOf::is_empty());
        assert_eq!(DequeuedOf::get(), 1);

        QueueOf::requeue(dispatch_5).expect("Algorithmic error");

        assert!(QueueProcessingOf::denied());
        assert_eq!(DequeuedOf::get(), 0);
        assert_eq!(QueueOf::len(), 1);
    });
}
