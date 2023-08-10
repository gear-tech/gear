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

use crate::{mock::*, *};
use common::GasPrice;
use frame_support::{assert_noop, assert_ok};
use sp_runtime::traits::{ConstU128, Zero};
use utils::*;

#[test]
fn deposit_gas_different_users() {
    new_test_ext().execute_with(|| {
        assert_bank_balance(0, 0);

        assert_balance(&ALICE, ALICE_BALANCE);
        assert_balance(&BOB, BOB_BALANCE);

        const ALICE_GAS: u64 = 1_234_567;
        assert_ok!(GearBank::deposit_gas::<GC>(&ALICE, ALICE_GAS));

        assert_bank_balance(ALICE_GAS, 0);

        assert_alice_dec(GasConverter::gas_price(ALICE_GAS));
        assert_gas_value(&ALICE, ALICE_GAS, 0);

        const BOB_GAS: u64 = 56_789;
        assert_ok!(GearBank::deposit_gas::<GC>(&BOB, BOB_GAS));

        assert_bank_balance(ALICE_GAS + BOB_GAS, 0);

        assert_alice_dec(GasConverter::gas_price(ALICE_GAS));
        assert_gas_value(&ALICE, ALICE_GAS, 0);

        assert_bob_dec(GasConverter::gas_price(BOB_GAS));
        assert_gas_value(&BOB, BOB_GAS, 0);
    })
}

#[test]
fn deposit_gas_single_user() {
    new_test_ext().execute_with(|| {
        const GAS_1: u64 = 123_456;
        assert_ok!(GearBank::deposit_gas::<GC>(&ALICE, GAS_1));

        assert_bank_balance(GAS_1, 0);

        assert_alice_dec(GasConverter::gas_price(GAS_1));
        assert_gas_value(&ALICE, GAS_1, 0);

        const GAS_2: u64 = 67_890;
        assert_ok!(GearBank::deposit_gas::<GC>(&ALICE, GAS_2));

        assert_bank_balance(GAS_1 + GAS_2, 0);

        assert_alice_dec(GasConverter::gas_price(GAS_1 + GAS_2));
        assert_gas_value(&ALICE, GAS_1 + GAS_2, 0);
    })
}

#[test]
fn deposit_gas_user_account_deleted() {
    new_test_ext().execute_with(|| {
        const ALICE_TO_DUST_BALANCE: Balance = EXISTENTIAL_DEPOSIT - VALUE_PER_GAS;

        const BALANCE_DIFF: Balance = ALICE_BALANCE - ALICE_TO_DUST_BALANCE;
        const GAS_AMOUNT: u64 = (BALANCE_DIFF / VALUE_PER_GAS) as u64;

        assert_eq!(GasConverter::gas_price(GAS_AMOUNT), BALANCE_DIFF);

        assert_ok!(GearBank::deposit_gas::<GC>(&ALICE, GAS_AMOUNT));

        assert_bank_balance(GAS_AMOUNT, 0);

        assert_balance(&ALICE, 0);
        assert_gas_value(&ALICE, GAS_AMOUNT, 0);
    })
}

#[test]
fn deposit_gas_zero() {
    new_test_ext().execute_with(|| {
        let h = frame_support::storage_root(frame_support::StateVersion::V1);

        assert_ok!(GearBank::deposit_gas::<GC>(&ALICE, 0));

        assert_ok!(GearBank::deposit_gas::<GC>(&Zero::zero(), 0));

        // No-op operation assertion.
        assert_eq!(
            h,
            frame_support::storage_root(frame_support::StateVersion::V1),
            "storage has been mutated"
        );
    })
}

#[test]
fn deposit_gas_insufficient_balance() {
    new_test_ext().execute_with(|| {
        const GAS_AMOUNT: u64 = u64::MAX;

        assert!(GasConverter::gas_price(GAS_AMOUNT) > Balances::free_balance(ALICE));

        assert_noop!(
            GearBank::deposit_gas::<GC>(&ALICE, GAS_AMOUNT),
            Error::<Test>::InsufficientBalance
        );
    })
}

#[test]
fn deposit_gas_insufficient_deposit() {
    // Unreachable case for Gear protocol.
    new_test_ext().execute_with(|| {
        const GAS_AMOUNT: u64 = 1;

        assert!(GasConverter::gas_price(GAS_AMOUNT) < CurrencyOf::<Test>::minimum_balance());

        assert_ok!(Balances::transfer_all(
            RuntimeOrigin::signed(BANK_ADDRESS),
            Zero::zero(),
            false,
        ));

        assert_noop!(
            GearBank::deposit_gas::<GC>(&ALICE, GAS_AMOUNT),
            Error::<Test>::InsufficientDeposit
        );
    })
}

#[test]
fn withdraw_gas_different_users() {
    new_test_ext().execute_with(|| {
        const ALICE_GAS: u64 = 1_234_567;
        assert_ok!(GearBank::deposit_gas::<GC>(&ALICE, ALICE_GAS));

        const BOB_GAS: u64 = 56_789;
        assert_ok!(GearBank::deposit_gas::<GC>(&BOB, BOB_GAS));

        const ALICE_WITHDRAW: u64 = ALICE_GAS - 123_456;
        assert_ok!(GearBank::withdraw_gas::<GC>(&ALICE, ALICE_WITHDRAW));

        assert_bank_balance(ALICE_GAS - ALICE_WITHDRAW + BOB_GAS, 0);

        assert_alice_dec(GasConverter::gas_price(ALICE_GAS - ALICE_WITHDRAW));
        assert_gas_value(&ALICE, ALICE_GAS - ALICE_WITHDRAW, 0);

        assert_bob_dec(GasConverter::gas_price(BOB_GAS));
        assert_gas_value(&BOB, BOB_GAS, 0);

        const BOB_WITHDRAW: u64 = BOB_GAS - 1_234;
        assert_ok!(GearBank::withdraw_gas::<GC>(&BOB, BOB_WITHDRAW));

        assert_bank_balance(ALICE_GAS - ALICE_WITHDRAW + BOB_GAS - BOB_WITHDRAW, 0);

        assert_alice_dec(GasConverter::gas_price(ALICE_GAS - ALICE_WITHDRAW));
        assert_gas_value(&ALICE, ALICE_GAS - ALICE_WITHDRAW, 0);

        assert_bob_dec(GasConverter::gas_price(BOB_GAS - BOB_WITHDRAW));
        assert_gas_value(&BOB, BOB_GAS - BOB_WITHDRAW, 0);
    })
}

#[test]
fn withdraw_gas_single_user() {
    new_test_ext().execute_with(|| {
        const GAS_AMOUNT: u64 = 123_456;
        assert_ok!(GearBank::deposit_gas::<GC>(&ALICE, GAS_AMOUNT));

        const WITHDRAW_1: u64 = GAS_AMOUNT - 23_456;
        assert_ok!(GearBank::withdraw_gas::<GC>(&ALICE, WITHDRAW_1));

        assert_bank_balance(GAS_AMOUNT - WITHDRAW_1, 0);

        assert_alice_dec(GasConverter::gas_price(GAS_AMOUNT - WITHDRAW_1));
        assert_gas_value(&ALICE, GAS_AMOUNT - WITHDRAW_1, 0);

        const WITHDRAW_2: u64 = GAS_AMOUNT - WITHDRAW_1 - 10_000;
        assert_ok!(GearBank::withdraw_gas::<GC>(&ALICE, WITHDRAW_2));

        assert_bank_balance(GAS_AMOUNT - WITHDRAW_1 - WITHDRAW_2, 0);

        assert_alice_dec(GasConverter::gas_price(
            GAS_AMOUNT - WITHDRAW_1 - WITHDRAW_2,
        ));
        assert_gas_value(&ALICE, GAS_AMOUNT - WITHDRAW_1 - WITHDRAW_2, 0);
    })
}

#[test]
fn withdraw_gas_all_balance() {
    new_test_ext().execute_with(|| {
        const GAS_AMOUNT: u64 = 123_456;
        assert_ok!(GearBank::deposit_gas::<GC>(&ALICE, GAS_AMOUNT));

        assert_ok!(GearBank::withdraw_gas::<GC>(&ALICE, GAS_AMOUNT));

        assert_bank_balance(0, 0);

        assert_alice_dec(0);
        assert_gas_value(&ALICE, 0, 0);
    })
}

#[test]
fn withdraw_gas_all_balance_user_account_deleted() {
    new_test_ext().execute_with(|| {
        const ALICE_TO_DUST_BALANCE: Balance = EXISTENTIAL_DEPOSIT - VALUE_PER_GAS;

        const BALANCE_DIFF: Balance = ALICE_BALANCE - ALICE_TO_DUST_BALANCE;
        const GAS_AMOUNT: u64 = (BALANCE_DIFF / VALUE_PER_GAS) as u64;

        assert_eq!(GasConverter::gas_price(GAS_AMOUNT), BALANCE_DIFF);
        assert!(BALANCE_DIFF > CurrencyOf::<Test>::minimum_balance());

        assert_ok!(GearBank::deposit_gas::<GC>(&ALICE, GAS_AMOUNT));
        assert_ok!(GearBank::withdraw_gas::<GC>(&ALICE, GAS_AMOUNT));

        assert_bank_balance(0, 0);

        assert_alice_dec(ALICE_TO_DUST_BALANCE);
        assert_gas_value(&ALICE, 0, 0);
    })
}

#[test]
fn withdraw_gas_small_amount() {
    new_test_ext().execute_with(|| {
        const GAS_AMOUNT: u64 = ((EXISTENTIAL_DEPOSIT - VALUE_PER_GAS) / VALUE_PER_GAS) as u64;

        assert!(GasConverter::gas_price(GAS_AMOUNT) < CurrencyOf::<Test>::minimum_balance());

        assert_ok!(GearBank::deposit_gas::<GC>(&ALICE, GAS_AMOUNT));

        assert_ok!(GearBank::withdraw_gas::<GC>(&ALICE, GAS_AMOUNT));

        assert_bank_balance(0, 0);

        assert_alice_dec(0);
        assert_gas_value(&ALICE, 0, 0);
    })
}

#[test]
fn withdraw_gas_small_amount_user_account_deleted() {
    new_test_ext().execute_with(|| {
        const GAS_VALUE_AMOUNT: Balance = EXISTENTIAL_DEPOSIT - VALUE_PER_GAS;
        assert!(GAS_VALUE_AMOUNT < CurrencyOf::<Test>::minimum_balance());

        const GAS_AMOUNT: u64 = (GAS_VALUE_AMOUNT / VALUE_PER_GAS) as u64;
        assert_eq!(GasConverter::gas_price(GAS_AMOUNT), GAS_VALUE_AMOUNT);

        assert_ok!(GearBank::deposit_gas::<GC>(&ALICE, GAS_AMOUNT));

        assert_ok!(Balances::transfer_all(
            RuntimeOrigin::signed(ALICE),
            Zero::zero(),
            false,
        ));

        assert_ok!(GearBank::withdraw_gas::<GC>(&ALICE, GAS_AMOUNT));

        assert_eq!(GearBank::unused_value(), GAS_VALUE_AMOUNT);
        assert_balance(&BANK_ADDRESS, EXISTENTIAL_DEPOSIT + GAS_VALUE_AMOUNT);

        assert_bank_balance(0, 0);

        assert_balance(&ALICE, 0);
        assert_gas_value(&ALICE, 0, 0);
    })
}

#[test]
fn withdraw_gas_zero() {
    new_test_ext().execute_with(|| {
        let h = frame_support::storage_root(frame_support::StateVersion::V1);

        assert_ok!(GearBank::withdraw_gas::<GC>(&ALICE, 0));

        assert_ok!(GearBank::withdraw_gas::<GC>(&Zero::zero(), 0));

        // No-op operation assertion.
        assert_eq!(
            h,
            frame_support::storage_root(frame_support::StateVersion::V1),
            "storage has been mutated"
        );
    })
}

#[test]
fn withdraw_gas_insufficient_bank_balance() {
    // Unreachable case for Gear protocol.
    new_test_ext().execute_with(|| {
        const GAS_AMOUNT: u64 = 123_456;

        assert_ok!(GearBank::deposit_gas::<GC>(&ALICE, GAS_AMOUNT));

        assert_ok!(Balances::transfer_all(
            RuntimeOrigin::signed(BANK_ADDRESS),
            Zero::zero(),
            false,
        ));

        assert_noop!(
            GearBank::withdraw_gas::<GC>(&ALICE, GAS_AMOUNT),
            Error::<Test>::InsufficientBankBalance
        );
    })
}

#[test]
fn withdraw_gas_insufficient_gas_balance() {
    new_test_ext().execute_with(|| {
        const GAS_AMOUNT: u64 = 123_456;

        assert_ok!(GearBank::deposit_gas::<GC>(&ALICE, GAS_AMOUNT));

        assert_noop!(
            GearBank::withdraw_gas::<GC>(&ALICE, GAS_AMOUNT + 1),
            Error::<Test>::InsufficientGasBalance
        );
    })
}

#[test]
fn withdraw_gas_insufficient_inexistent_gas_balance() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            GearBank::withdraw_gas::<GC>(&ALICE, 1),
            Error::<Test>::InsufficientGasBalance
        );

        assert_noop!(
            GearBank::withdraw_gas::<GC>(&Zero::zero(), 1),
            Error::<Test>::InsufficientGasBalance
        );
    })
}

#[test]
fn spend_gas_different_users() {
    new_test_ext().execute_with(|| {
        const ALICE_GAS: u64 = 1_234_567;
        assert_ok!(GearBank::deposit_gas::<GC>(&ALICE, ALICE_GAS));

        const BOB_GAS: u64 = 56_789;
        assert_ok!(GearBank::deposit_gas::<GC>(&BOB, BOB_GAS));

        const ALICE_BURN: u64 = ALICE_GAS - 123_456;
        assert_ok!(GearBank::spend_gas::<GC>(&ALICE, ALICE_BURN));

        assert_bank_balance(ALICE_GAS - ALICE_BURN + BOB_GAS, 0);

        assert_block_author_inc(GasConverter::gas_price(ALICE_BURN));

        assert_alice_dec(GasConverter::gas_price(ALICE_GAS));
        assert_gas_value(&ALICE, ALICE_GAS - ALICE_BURN, 0);

        assert_bob_dec(GasConverter::gas_price(BOB_GAS));
        assert_gas_value(&BOB, BOB_GAS, 0);

        const BOB_BURN: u64 = BOB_GAS - 1_234;
        assert_ok!(GearBank::spend_gas::<GC>(&BOB, BOB_BURN));

        assert_bank_balance(ALICE_GAS - ALICE_BURN + BOB_GAS - BOB_BURN, 0);

        assert_block_author_inc(GasConverter::gas_price(ALICE_BURN + BOB_BURN));

        assert_alice_dec(GasConverter::gas_price(ALICE_GAS));
        assert_gas_value(&ALICE, ALICE_GAS - ALICE_BURN, 0);

        assert_bob_dec(GasConverter::gas_price(BOB_GAS));
        assert_gas_value(&BOB, BOB_GAS - BOB_BURN, 0);
    })
}

#[test]
fn spend_gas_single_user() {
    new_test_ext().execute_with(|| {
        const GAS_AMOUNT: u64 = 123_456;
        assert_ok!(GearBank::deposit_gas::<GC>(&ALICE, GAS_AMOUNT));

        const BURN_1: u64 = GAS_AMOUNT - 23_456;
        assert_ok!(GearBank::spend_gas::<GC>(&ALICE, BURN_1));

        assert_bank_balance(GAS_AMOUNT - BURN_1, 0);

        assert_block_author_inc(GasConverter::gas_price(BURN_1));

        assert_alice_dec(GasConverter::gas_price(GAS_AMOUNT));
        assert_gas_value(&ALICE, GAS_AMOUNT - BURN_1, 0);

        const BURN_2: u64 = GAS_AMOUNT - BURN_1 - 10_000;
        assert_ok!(GearBank::spend_gas::<GC>(&ALICE, BURN_2));

        assert_bank_balance(GAS_AMOUNT - BURN_1 - BURN_2, 0);

        assert_block_author_inc(GasConverter::gas_price(BURN_1 + BURN_2));

        assert_alice_dec(GasConverter::gas_price(GAS_AMOUNT));
        assert_gas_value(&ALICE, GAS_AMOUNT - BURN_1 - BURN_2, 0);
    })
}

#[test]
fn spend_gas_all_balance() {
    new_test_ext().execute_with(|| {
        const GAS_AMOUNT: u64 = 123_456;
        assert_ok!(GearBank::deposit_gas::<GC>(&ALICE, GAS_AMOUNT));

        assert_ok!(GearBank::spend_gas::<GC>(&ALICE, GAS_AMOUNT));

        assert_bank_balance(0, 0);

        assert_block_author_inc(GasConverter::gas_price(GAS_AMOUNT));

        assert_alice_dec(GasConverter::gas_price(GAS_AMOUNT));
        assert_gas_value(&ALICE, 0, 0);
    })
}

#[test]
fn spend_gas_all_balance_validator_account_deleted() {
    new_test_ext().execute_with(|| {
        const GAS_AMOUNT: u64 = 123_456;

        assert!(GasConverter::gas_price(GAS_AMOUNT) >= CurrencyOf::<Test>::minimum_balance());

        assert_ok!(GearBank::deposit_gas::<GC>(&ALICE, GAS_AMOUNT));

        assert_ok!(Balances::transfer_all(
            RuntimeOrigin::signed(BLOCK_AUTHOR),
            Zero::zero(),
            false,
        ));

        assert_ok!(GearBank::spend_gas::<GC>(&ALICE, GAS_AMOUNT));

        assert_bank_balance(0, 0);

        assert_balance(&BLOCK_AUTHOR, GasConverter::gas_price(GAS_AMOUNT));

        assert_alice_dec(GasConverter::gas_price(GAS_AMOUNT));
        assert_gas_value(&ALICE, 0, 0);
    })
}

#[test]
fn spend_gas_small_amount() {
    new_test_ext().execute_with(|| {
        const GAS_AMOUNT: u64 = ((EXISTENTIAL_DEPOSIT - VALUE_PER_GAS) / VALUE_PER_GAS) as u64;

        assert!(GasConverter::gas_price(GAS_AMOUNT) < CurrencyOf::<Test>::minimum_balance());

        assert_ok!(GearBank::deposit_gas::<GC>(&ALICE, GAS_AMOUNT));

        assert_ok!(GearBank::spend_gas::<GC>(&ALICE, GAS_AMOUNT));

        assert_bank_balance(0, 0);

        assert_block_author_inc(GasConverter::gas_price(GAS_AMOUNT));

        assert_alice_dec(GasConverter::gas_price(GAS_AMOUNT));
        assert_gas_value(&ALICE, 0, 0);
    })
}

#[test]
fn spend_gas_small_amount_validator_account_deleted() {
    new_test_ext().execute_with(|| {
        const GAS_VALUE_AMOUNT: Balance = EXISTENTIAL_DEPOSIT - VALUE_PER_GAS;
        assert!(GAS_VALUE_AMOUNT < CurrencyOf::<Test>::minimum_balance());

        const GAS_AMOUNT: u64 = (GAS_VALUE_AMOUNT / VALUE_PER_GAS) as u64;
        assert_eq!(GasConverter::gas_price(GAS_AMOUNT), GAS_VALUE_AMOUNT);

        assert_ok!(GearBank::deposit_gas::<GC>(&ALICE, GAS_AMOUNT));

        assert_ok!(Balances::transfer_all(
            RuntimeOrigin::signed(BLOCK_AUTHOR),
            Zero::zero(),
            false,
        ));

        assert_ok!(GearBank::spend_gas::<GC>(&ALICE, GAS_AMOUNT));

        assert_eq!(GearBank::unused_value(), GAS_VALUE_AMOUNT);
        assert_balance(&BANK_ADDRESS, EXISTENTIAL_DEPOSIT + GAS_VALUE_AMOUNT);

        assert_bank_balance(0, 0);

        assert_balance(&BLOCK_AUTHOR, 0);

        assert_alice_dec(GasConverter::gas_price(GAS_AMOUNT));
        assert_gas_value(&ALICE, 0, 0);
    })
}

#[test]
fn spend_gas_zero() {
    new_test_ext().execute_with(|| {
        let h = frame_support::storage_root(frame_support::StateVersion::V1);

        assert_ok!(GearBank::spend_gas::<GC>(&ALICE, 0));

        assert_ok!(GearBank::spend_gas::<GC>(&Zero::zero(), 0));

        // No-op operation assertion.
        assert_eq!(
            h,
            frame_support::storage_root(frame_support::StateVersion::V1),
            "storage has been mutated"
        );
    })
}

#[test]
fn spend_gas_insufficient_bank_balance() {
    // Unreachable case for Gear protocol.
    new_test_ext().execute_with(|| {
        const GAS_AMOUNT: u64 = 123_456;

        assert_ok!(GearBank::deposit_gas::<GC>(&ALICE, GAS_AMOUNT));

        assert_ok!(Balances::transfer_all(
            RuntimeOrigin::signed(BANK_ADDRESS),
            Zero::zero(),
            false,
        ));

        assert_balance(&BANK_ADDRESS, 0);

        assert_noop!(
            GearBank::spend_gas::<GC>(&ALICE, GAS_AMOUNT),
            Error::<Test>::InsufficientBankBalance
        );
    })
}

#[test]
fn spend_gas_insufficient_gas_balance() {
    new_test_ext().execute_with(|| {
        const GAS_AMOUNT: u64 = 123_456;

        assert_ok!(GearBank::deposit_gas::<GC>(&ALICE, GAS_AMOUNT));

        assert_noop!(
            GearBank::spend_gas::<GC>(&ALICE, GAS_AMOUNT + 1),
            Error::<Test>::InsufficientGasBalance
        );
    })
}

#[test]
fn spend_gas_insufficient_inexistent_gas_balance() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            GearBank::spend_gas::<GC>(&ALICE, 1),
            Error::<Test>::InsufficientGasBalance
        );

        assert_noop!(
            GearBank::spend_gas::<GC>(&Zero::zero(), 1),
            Error::<Test>::InsufficientGasBalance
        );
    })
}

#[test]
fn deposit_value_different_users() {
    new_test_ext().execute_with(|| {
        const ALICE_VALUE: Balance = 1_234_567_000;
        assert_ok!(GearBank::deposit_value(&ALICE, ALICE_VALUE));

        assert_bank_balance(0, ALICE_VALUE);

        assert_alice_dec(ALICE_VALUE);
        assert_gas_value(&ALICE, 0, ALICE_VALUE);

        const BOB_VALUE: Balance = 56_789_000;
        assert_ok!(GearBank::deposit_value(&BOB, BOB_VALUE));

        assert_bank_balance(0, ALICE_VALUE + BOB_VALUE);

        assert_alice_dec(ALICE_VALUE);
        assert_gas_value(&ALICE, 0, ALICE_VALUE);

        assert_bob_dec(BOB_VALUE);
        assert_gas_value(&BOB, 0, BOB_VALUE);
    })
}

#[test]
fn deposit_value_single_user() {
    new_test_ext().execute_with(|| {
        const VALUE_1: Balance = 123_456_000;
        assert_ok!(GearBank::deposit_value(&ALICE, VALUE_1));

        assert_bank_balance(0, VALUE_1);

        assert_alice_dec(VALUE_1);
        assert_gas_value(&ALICE, 0, VALUE_1);

        const VALUE_2: Balance = 67_890_000;
        assert_ok!(GearBank::deposit_value(&ALICE, VALUE_2));

        assert_bank_balance(0, VALUE_1 + VALUE_2);

        assert_alice_dec(VALUE_1 + VALUE_2);
        assert_gas_value(&ALICE, 0, VALUE_1 + VALUE_2);
    })
}

#[test]
fn deposit_value_user_account_deleted() {
    new_test_ext().execute_with(|| {
        const ALICE_TO_DUST_BALANCE: Balance = EXISTENTIAL_DEPOSIT - 1;

        const VALUE: Balance = ALICE_BALANCE - ALICE_TO_DUST_BALANCE;

        assert_ok!(GearBank::deposit_value(&ALICE, VALUE));

        assert_bank_balance(0, VALUE);

        assert_balance(&ALICE, 0);
        assert_gas_value(&ALICE, 0, VALUE);
    })
}

#[test]
fn deposit_value_zero() {
    new_test_ext().execute_with(|| {
        let h = frame_support::storage_root(frame_support::StateVersion::V1);

        assert_ok!(GearBank::deposit_value(&ALICE, 0));

        assert_ok!(GearBank::deposit_value(&Zero::zero(), 0));

        // No-op operation assertion.
        assert_eq!(
            h,
            frame_support::storage_root(frame_support::StateVersion::V1),
            "storage has been mutated"
        );
    })
}

#[test]
fn deposit_value_insufficient_balance() {
    new_test_ext().execute_with(|| {
        const VALUE: Balance = Balance::MAX;

        assert!(VALUE > Balances::free_balance(ALICE));

        assert_noop!(
            GearBank::deposit_value(&ALICE, VALUE),
            Error::<Test>::InsufficientBalance
        );
    })
}

#[test]
fn deposit_value_insufficient_deposit() {
    // Unreachable case for Gear protocol.
    new_test_ext().execute_with(|| {
        const VALUE: Balance = EXISTENTIAL_DEPOSIT - 1;

        assert_ok!(Balances::transfer_all(
            RuntimeOrigin::signed(BANK_ADDRESS),
            Zero::zero(),
            false,
        ));

        assert_noop!(
            GearBank::deposit_value(&ALICE, VALUE),
            Error::<Test>::InsufficientDeposit
        );
    })
}

#[test]
fn withdraw_value_different_users() {
    new_test_ext().execute_with(|| {
        const ALICE_VALUE: Balance = 1_234_567_000;
        assert_ok!(GearBank::deposit_value(&ALICE, ALICE_VALUE));

        const BOB_VALUE: Balance = 56_789_000;
        assert_ok!(GearBank::deposit_value(&BOB, BOB_VALUE));

        const ALICE_WITHDRAW: Balance = ALICE_VALUE - 123_456_000;
        assert_ok!(GearBank::withdraw_value(&ALICE, ALICE_WITHDRAW));

        assert_bank_balance(0, ALICE_VALUE - ALICE_WITHDRAW + BOB_VALUE);

        assert_alice_dec(ALICE_VALUE - ALICE_WITHDRAW);
        assert_gas_value(&ALICE, 0, ALICE_VALUE - ALICE_WITHDRAW);

        assert_bob_dec(BOB_VALUE);
        assert_gas_value(&BOB, 0, BOB_VALUE);

        const BOB_WITHDRAW: Balance = BOB_VALUE - 1_234_000;
        assert_ok!(GearBank::withdraw_value(&BOB, BOB_WITHDRAW));

        assert_bank_balance(0, ALICE_VALUE - ALICE_WITHDRAW + BOB_VALUE - BOB_WITHDRAW);

        assert_alice_dec(ALICE_VALUE - ALICE_WITHDRAW);
        assert_gas_value(&ALICE, 0, ALICE_VALUE - ALICE_WITHDRAW);

        assert_bob_dec(BOB_VALUE - BOB_WITHDRAW);
        assert_gas_value(&BOB, 0, BOB_VALUE - BOB_WITHDRAW);
    })
}

#[test]
fn withdraw_value_single_user() {
    new_test_ext().execute_with(|| {
        const VALUE: Balance = 123_456_000;
        assert_ok!(GearBank::deposit_value(&ALICE, VALUE));

        const WITHDRAW_1: Balance = VALUE - 23_456_000;
        assert_ok!(GearBank::withdraw_value(&ALICE, WITHDRAW_1));

        assert_bank_balance(0, VALUE - WITHDRAW_1);

        assert_alice_dec(VALUE - WITHDRAW_1);
        assert_gas_value(&ALICE, 0, VALUE - WITHDRAW_1);

        const WITHDRAW_2: Balance = VALUE - WITHDRAW_1 - 10_000_000;
        assert_ok!(GearBank::withdraw_value(&ALICE, WITHDRAW_2));

        assert_bank_balance(0, VALUE - WITHDRAW_1 - WITHDRAW_2);

        assert_alice_dec(VALUE - WITHDRAW_1 - WITHDRAW_2);
        assert_gas_value(&ALICE, 0, VALUE - WITHDRAW_1 - WITHDRAW_2);
    })
}

#[test]
fn withdraw_value_all_balance() {
    new_test_ext().execute_with(|| {
        const VALUE: Balance = 123_456_000;
        assert_ok!(GearBank::deposit_value(&ALICE, VALUE));

        assert_ok!(GearBank::withdraw_value(&ALICE, VALUE));

        assert_bank_balance(0, 0);

        assert_alice_dec(0);
        assert_gas_value(&ALICE, 0, 0);
    })
}

#[test]
fn withdraw_value_all_balance_user_account_deleted() {
    new_test_ext().execute_with(|| {
        const ALICE_TO_DUST_BALANCE: Balance = EXISTENTIAL_DEPOSIT - 1;

        const VALUE: Balance = ALICE_BALANCE - ALICE_TO_DUST_BALANCE;

        assert_ok!(GearBank::deposit_value(&ALICE, VALUE));
        assert_ok!(GearBank::withdraw_value(&ALICE, VALUE));

        assert_bank_balance(0, 0);

        assert_alice_dec(ALICE_TO_DUST_BALANCE);
        assert_gas_value(&ALICE, 0, 0);
    })
}

#[test]
fn withdraw_value_small_amount() {
    new_test_ext().execute_with(|| {
        const VALUE: u128 = EXISTENTIAL_DEPOSIT - 1;

        assert_ok!(GearBank::deposit_value(&ALICE, VALUE));

        assert_ok!(GearBank::withdraw_value(&ALICE, VALUE));

        assert_bank_balance(0, 0);

        assert_alice_dec(0);
        assert_gas_value(&ALICE, 0, 0);
    })
}

#[test]
fn withdraw_value_small_amount_user_account_deleted() {
    new_test_ext().execute_with(|| {
        const VALUE: Balance = EXISTENTIAL_DEPOSIT - 1;

        assert_ok!(GearBank::deposit_value(&ALICE, VALUE));

        assert_ok!(Balances::transfer_all(
            RuntimeOrigin::signed(ALICE),
            Zero::zero(),
            false,
        ));

        assert_ok!(GearBank::withdraw_value(&ALICE, VALUE));

        assert_eq!(GearBank::unused_value(), VALUE);
        assert_balance(&BANK_ADDRESS, EXISTENTIAL_DEPOSIT + VALUE);

        assert_bank_balance(0, 0);

        assert_balance(&ALICE, 0);
        assert_gas_value(&ALICE, 0, 0);
    })
}

#[test]
fn withdraw_value_zero() {
    new_test_ext().execute_with(|| {
        let h = frame_support::storage_root(frame_support::StateVersion::V1);

        assert_ok!(GearBank::withdraw_value(&ALICE, 0));

        assert_ok!(GearBank::withdraw_value(&Zero::zero(), 0));

        // No-op operation assertion.
        assert_eq!(
            h,
            frame_support::storage_root(frame_support::StateVersion::V1),
            "storage has been mutated"
        );
    })
}

#[test]
fn withdraw_value_insufficient_bank_balance() {
    // Unreachable case for Gear protocol.
    new_test_ext().execute_with(|| {
        const VALUE: Balance = 123_456_000;

        assert_ok!(GearBank::deposit_value(&ALICE, VALUE));

        assert_ok!(Balances::transfer_all(
            RuntimeOrigin::signed(BANK_ADDRESS),
            Zero::zero(),
            false,
        ));

        assert_noop!(
            GearBank::withdraw_value(&ALICE, VALUE),
            Error::<Test>::InsufficientBankBalance
        );
    })
}

#[test]
fn withdraw_value_insufficient_value_balance() {
    new_test_ext().execute_with(|| {
        const VALUE: Balance = 123_456_000;

        assert_ok!(GearBank::deposit_value(&ALICE, VALUE));

        assert_noop!(
            GearBank::withdraw_value(&ALICE, VALUE + 1),
            Error::<Test>::InsufficientValueBalance
        );
    })
}

#[test]
fn withdraw_value_insufficient_inexistent_value_balance() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            GearBank::withdraw_value(&ALICE, 1),
            Error::<Test>::InsufficientValueBalance
        );

        assert_noop!(
            GearBank::withdraw_value(&Zero::zero(), 1),
            Error::<Test>::InsufficientValueBalance
        );
    })
}

mod utils {
    use super::*;

    pub const VALUE_PER_GAS: u128 = 1_000;

    pub struct GasConverter;
    impl common::GasPrice for GasConverter {
        type Balance = Balance;
        type GasToBalanceMultiplier = ConstU128<VALUE_PER_GAS>;
    }

    pub type GC = GasConverter;

    // For some reason `assert_noop!` doesnt work for the pallet fns.
    impl PartialEq for Error<Test> {
        fn eq(&self, other: &Self) -> bool {
            match self {
                Self::InsufficientBalance => matches!(other, Self::InsufficientBalance),
                Self::InsufficientBankBalance => matches!(other, Self::InsufficientBankBalance),
                Self::InsufficientGasBalance => matches!(other, Self::InsufficientGasBalance),
                Self::InsufficientValueBalance => matches!(other, Self::InsufficientValueBalance),
                Self::InsufficientDeposit => matches!(other, Self::InsufficientDeposit),
                _ => unimplemented!(),
            }
        }
    }

    #[track_caller]
    pub fn assert_balance(account_id: &AccountId, value: Balance) {
        assert_eq!(Balances::total_balance(account_id), value);
        assert!(Balances::reserved_balance(account_id).is_zero());
    }

    #[track_caller]
    pub fn assert_bank_balance(gas: u64, value: Balance) {
        let gas_value = GasConverter::gas_price(gas);
        assert_balance(
            &BANK_ADDRESS,
            CurrencyOf::<Test>::minimum_balance() + GearBank::unused_value() + gas_value + value,
        );
    }

    #[track_caller]
    pub fn assert_gas_value(account_id: &AccountId, expected_gas: u64, expected_value: Balance) {
        let BankAccount { gas, value } = GearBank::account(account_id).unwrap_or_default();

        assert_eq!(gas, GasConverter::gas_price(expected_gas));
        assert_eq!(value, expected_value);
    }

    // Asserts Alice balance decrease.
    #[track_caller]
    pub fn assert_alice_dec(diff: Balance) {
        assert_balance(&ALICE, ALICE_BALANCE - diff)
    }

    // Asserts Bob balance decrease.
    #[track_caller]
    pub fn assert_bob_dec(diff: Balance) {
        assert_balance(&BOB, BOB_BALANCE - diff)
    }

    // Asserts block author balance inc.
    #[track_caller]
    pub fn assert_block_author_inc(diff: Balance) {
        assert_balance(&BLOCK_AUTHOR, EXISTENTIAL_DEPOSIT + diff)
    }
}
