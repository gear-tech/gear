// This file is part of Gear.

// Copyright (C) 2024 Gear Technologies Inc.
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

use crate::{mock::*, Hasher};
use binary_merkle_tree as merkle_tree;
use frame_support::{assert_noop, assert_ok};
use primitive_types::H256;

type Queue = crate::Queue<Test>;
type Event = crate::Event<Test>;
type Error = crate::Error<Test>;
type QueueLimit = <Test as crate::Config>::QueueLimit;

#[test]
fn on_finalize_noop_on_empty_queue() {
    init_logger();
    new_test_ext().execute_with(|| {
        run_to_next_block();

        assert!(System::events().is_empty());
    })
}

#[test]
fn send_deposits_event_and_appends_queue() {
    init_logger();
    new_test_ext().execute_with(|| {
        const TEST_CASES: usize = 5;

        assert!(Queue::get().is_none());

        let mut hashes = vec![];

        for _ in 0..TEST_CASES {
            let hash = H256::random();
            hashes.push(hash);

            assert_ok!(GearBridge::send(RuntimeOrigin::signed(USER), hash));
            System::assert_last_event(Event::MessageQueued(hash).into());
            assert_eq!(Queue::get().unwrap(), hashes);
        }
    })
}

#[test]
fn send_returns_error_on_overflow() {
    init_logger();
    new_test_ext().execute_with(|| {
        let hash = H256::random();

        for _ in 0..QueueLimit::get() {
            assert_ok!(GearBridge::send(RuntimeOrigin::signed(USER), hash));
        }

        assert_noop!(
            GearBridge::send(RuntimeOrigin::signed(USER), hash),
            Error::QueueLimitExceeded
        );
    })
}

#[test]
fn on_finalize_works_with_multiple_element() {
    init_logger();
    new_test_ext().execute_with(|| {
        const TEST_CASES: usize = 5;

        assert!(Queue::get().is_none());

        let mut hashes = vec![];

        for _ in 0..TEST_CASES {
            let hash = H256::random();
            hashes.push(hash);

            assert_ok!(GearBridge::send(RuntimeOrigin::signed(USER), hash));

            run_to_next_block();
            let expected = merkle_tree::merkle_root::<Hasher, _>(&hashes);
            System::assert_last_event(Event::RootUpdated(expected).into());
        }
    })
}
