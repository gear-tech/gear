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

use super::*;
use crate::mock::{
    new_test_ext, Airdrop, AirdropCall, Balances, RuntimeCall, RuntimeOrigin, Sudo, ALICE, ROOT,
};
use frame_support::{assert_noop, assert_ok};

#[test]
fn test_setup_works() {
    new_test_ext().execute_with(|| {
        assert_eq!(Sudo::key(), Some(ROOT));
        assert_eq!(Balances::total_issuance(), 100_000_000);
    });
}

#[test]
fn sudo_call_works() {
    new_test_ext().execute_with(|| {
        let call = Box::new(RuntimeCall::Airdrop(AirdropCall::transfer {
            source: ROOT,
            dest: ALICE,
            amount: 10_000_000,
        }));
        assert_ok!(Sudo::sudo(RuntimeOrigin::signed(ROOT), call));
        assert_eq!(Balances::total_balance(&ALICE), 10_000_000);
        assert_eq!(Balances::total_balance(&ROOT), 90_000_000);
        assert_eq!(Balances::total_issuance(), 100_000_000);
    });
}

#[test]
fn signed_extrinsic_fails() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            Airdrop::transfer(RuntimeOrigin::signed(ROOT), ROOT, ALICE, 10_000_000_u128),
            DispatchError::BadOrigin,
        );
    });
}
