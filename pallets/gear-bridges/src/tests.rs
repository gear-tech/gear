// This file is part of Gear.

// Copyright (C) 2021-2023 Gear Technologies Inc.
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

use crate::{mock::*, Error};
use frame_support::{assert_noop, assert_ok, traits::Hooks, weights::Weight};

impl<T> PartialEq for Error<T> {
    fn eq(&self, other: &Self) -> bool {
        match self {
            Self::QueueOverflow => matches!(other, Error::<T>::QueueOverflow),
            Self::__Ignore(_, _) => unimplemented!(),
        }
    }
}

#[test]
fn can_submit_messages_up_to_max_queue_size() {
    new_test_ext().execute_with(|| {
        for _ in 0..MaxQueueLength::get() {
            assert_ok!(GearBridges::submit_message(&[0; 1]));
        }

        assert_noop!(
            GearBridges::submit_message(&[0; 1]),
            Error::<Test>::QueueOverflow
        );
    })
}

#[test]
fn correct_message_movement_order() {
    let _ = new_test_ext().execute_with(|| {
        let messages = (0..MaxQueueLength::get())
            .into_iter()
            .map(|n| n.to_le_bytes())
            .collect::<Vec<_>>();

        for message in &messages {
            assert_ok!(GearBridges::submit_message(message));
        }

        assert_eq!(GearBridges::pending_bridging(), None);

        for i in 0..MaxQueueLength::get() {
            GearBridges::on_idle((i + 1).into(), Weight::from_parts(100_000, 100_000));
            let message_hash =
                <<Test as crate::pallet::Config>::Hasher as sp_runtime::traits::Hash>::hash(
                    &messages[i as usize],
                );
            assert_eq!(Some(message_hash), GearBridges::pending_bridging());
        }
    });
}
