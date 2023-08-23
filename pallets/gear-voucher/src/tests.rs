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

use super::*;
use crate::mock::*;
use common::Origin;
use frame_support::{assert_noop, assert_ok};
use primitive_types::H256;

#[test]
fn voucher_issue_works() {
    new_test_ext().execute_with(|| {
        let program_id = ProgramId::from_origin(H256::from(b"some//quasy//random//program//id"));
        let synthesized = Voucher::voucher_account_id(&BOB, &program_id);

        assert_ok!(Voucher::issue(
            RuntimeOrigin::signed(ALICE),
            BOB,
            program_id,
            1_000,
        ));

        assert_ok!(Voucher::issue(
            RuntimeOrigin::signed(ALICE),
            BOB,
            program_id,
            2_000,
        ));

        assert_eq!(Balances::free_balance(synthesized), 3_000);

        // Insufficient funds
        assert_noop!(
            Voucher::issue(RuntimeOrigin::signed(ALICE), BOB, program_id, 100_000_000,),
            Error::<Test>::FailureToCreateVoucher
        );
    });
}

#[test]
fn voucher_redemption_works() {
    new_test_ext().execute_with(|| {
        let program_id = ProgramId::from_origin(H256::from(b"some//quasy//random//program//id"));
        let synthesized = Voucher::voucher_account_id(&BOB, &program_id);

        assert_ok!(Voucher::issue(
            RuntimeOrigin::signed(ALICE),
            BOB,
            program_id,
            5_000,
        ));

        assert_eq!(Balances::free_balance(synthesized), 5_000);

        // Redemption ok
        assert_ok!(Balances::reserve(
            &Voucher::voucher_id(BOB, program_id),
            2_000
        ));

        // Redemption fails
        assert_noop!(
            Balances::reserve(&Voucher::voucher_id(BOB, program_id), 100_000_000),
            pallet_balances::Error::<Test>::InsufficientBalance
        );
    });
}
