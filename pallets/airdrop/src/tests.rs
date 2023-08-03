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
use crate::mock::{
    new_test_ext, Airdrop, AirdropCall, AirdropError, Balances, RuntimeCall, RuntimeOrigin, Sudo,
    Test, Vesting, VestingError, ALICE, BOB, ROOT,
};
use frame_support::{assert_err, assert_noop, assert_ok};
use frame_system::Config;
use pallet_vesting::VestingInfo;

#[test]
fn test_setup_works() {
    new_test_ext().execute_with(|| {
        assert_eq!(Sudo::key(), Some(ROOT));
        assert_eq!(Balances::total_issuance(), 200_000_000);
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
        assert_eq!(Balances::total_issuance(), 200_000_000);

        assert_eq!(Balances::locks(BOB).len(), 1);
        let call = Box::new(RuntimeCall::Airdrop(AirdropCall::transfer_vested {
            source: BOB,
            dest: ALICE,
            schedule_index: 0,
            amount: None,
        }));
        assert_ok!(Sudo::sudo(RuntimeOrigin::signed(ROOT), call));
        assert_eq!(Balances::total_balance(&BOB), 0);
        assert_eq!(Balances::locks(BOB), vec![]);
        assert_eq!(Balances::total_balance(&ALICE), 110_000_000);
        assert_eq!(Balances::total_issuance(), 200_000_000);
    });
}
#[test]
fn vesting_transfer_works() {
    new_test_ext().execute_with(|| {
        assert_eq!(Balances::locks(BOB).len(), 1);
        assert_eq!(
            Vesting::vesting(BOB).unwrap().first().unwrap(),
            &VestingInfo::<VestingBalanceOf<Test>, <Test as Config>::BlockNumber>::new(
                100_000_000,
                100_000,
                100,
            )
        );
        assert_eq!(Balances::total_balance(&ALICE), 0);
        assert_eq!(Balances::total_balance(&BOB), 100_000_000);
        assert_eq!(Balances::total_balance(&ROOT), 100_000_000);
        assert_eq!(Balances::total_issuance(), 200_000_000);

        // Vesting must exist on the source account
        assert_err!(
            Airdrop::transfer_vested(RuntimeOrigin::root(), ALICE, BOB, 1, Some(200_000_000)),
            VestingError::NotVesting
        );

        // Schedule must exist on the source account
        assert_err!(
            Airdrop::transfer_vested(RuntimeOrigin::root(), BOB, ALICE, 1, Some(200_000_000)),
            VestingError::ScheduleIndexOutOfBounds
        );

        // Amount can't be bigger than locked funds
        assert_err!(
            Airdrop::transfer_vested(RuntimeOrigin::root(), BOB, ALICE, 0, Some(200_000_000)),
            AirdropError::AmountBigger
        );

        // Transfer part of vested funds to ALICE
        assert_ok!(Airdrop::transfer_vested(
            RuntimeOrigin::root(),
            BOB,
            ALICE,
            0,
            Some(10_000_000)
        ));

        // Check that BOB have the same vesting schedule reduced by unlocked funds
        assert_eq!(
            Vesting::vesting(BOB).unwrap().first().unwrap(),
            &VestingInfo::<VestingBalanceOf<Test>, <Test as Config>::BlockNumber>::new(
                90_000_000, 90_000, 100,
            )
        );
        assert_eq!(Balances::total_balance(&BOB), 90_000_000);
        assert_eq!(Balances::free_balance(ALICE), 10_000_000);
        assert_eq!(Balances::total_issuance(), 200_000_000);

        // Transfer all of vested funds to ALICE
        assert_ok!(Airdrop::transfer_vested(
            RuntimeOrigin::root(),
            BOB,
            ALICE,
            0,
            None
        ));

        // Check that BOB have no vesting and ALICE have all the unlocked funds.
        assert_eq!(Vesting::vesting(BOB), None);
        assert_eq!(Balances::total_balance(&BOB), 0);
        assert_eq!(Balances::free_balance(ALICE), 100_000_000);
        assert_eq!(Balances::total_issuance(), 200_000_000);
    });
}

#[test]
fn signed_extrinsic_fails() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            Airdrop::transfer(RuntimeOrigin::signed(ROOT), ROOT, ALICE, 10_000_000_u128),
            DispatchError::BadOrigin,
        );
        assert_noop!(
            Airdrop::transfer_vested(RuntimeOrigin::signed(ROOT), BOB, ALICE, 0, None),
            DispatchError::BadOrigin,
        );
    });
}
