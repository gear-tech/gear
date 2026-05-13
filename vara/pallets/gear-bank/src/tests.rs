// This file is part of Gear.

// Copyright (C) 2021-2025 Gear Technologies Inc.
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

use crate::{GasMultiplier, OnFinalizeValue, UnusedValue, mock::*, *};
use frame_support::{
    assert_noop, assert_ok,
    storage::{generator::StorageValue, unhashed},
    traits::Hooks,
};
use sp_runtime::{Percent, StateVersion, traits::Zero};
use utils::*;

#[test]
fn keep_alive_fails_deposits_on_low_balance() {
    new_test_ext().execute_with(|| {
        const ALICE_TO_DUST_BALANCE: Balance = EXISTENTIAL_DEPOSIT - VALUE_PER_GAS;

        const VALUE: Balance = ALICE_BALANCE - ALICE_TO_DUST_BALANCE;
        const GAS_AMOUNT: u64 = (VALUE / VALUE_PER_GAS) as u64;

        assert_noop!(
            GearBank::deposit_gas(&ALICE, GAS_AMOUNT, true),
            Error::<Test>::InsufficientBalance,
        );

        assert_noop!(
            GearBank::deposit_value(&ALICE, VALUE, true),
            Error::<Test>::InsufficientBalance,
        );
    })
}

#[test]
fn deposit_gas_different_users() {
    new_test_ext().execute_with(|| {
        assert_bank_balance(0, 0);

        assert_balance(&ALICE, ALICE_BALANCE);
        assert_balance(&BOB, BOB_BALANCE);

        const ALICE_GAS: u64 = 1_234_567;
        assert_ok!(GearBank::deposit_gas(&ALICE, ALICE_GAS, false));

        assert_bank_balance(ALICE_GAS, 0);

        assert_alice_dec(gas_price(ALICE_GAS));
        assert_gas_value(&ALICE, ALICE_GAS, 0);

        const BOB_GAS: u64 = 56_789;
        assert_ok!(GearBank::deposit_gas(&BOB, BOB_GAS, false));

        assert_bank_balance(ALICE_GAS + BOB_GAS, 0);

        assert_alice_dec(gas_price(ALICE_GAS));
        assert_gas_value(&ALICE, ALICE_GAS, 0);

        assert_bob_dec(gas_price(BOB_GAS));
        assert_gas_value(&BOB, BOB_GAS, 0);
    })
}

#[test]
fn deposit_gas_single_user() {
    new_test_ext().execute_with(|| {
        const GAS_1: u64 = 123_456;
        assert_ok!(GearBank::deposit_gas(&ALICE, GAS_1, false));

        assert_bank_balance(GAS_1, 0);

        assert_alice_dec(gas_price(GAS_1));
        assert_gas_value(&ALICE, GAS_1, 0);

        const GAS_2: u64 = 67_890;
        assert_ok!(GearBank::deposit_gas(&ALICE, GAS_2, false));

        assert_bank_balance(GAS_1 + GAS_2, 0);

        assert_alice_dec(gas_price(GAS_1 + GAS_2));
        assert_gas_value(&ALICE, GAS_1 + GAS_2, 0);
    })
}

#[test]
fn deposit_gas_user_account_deleted() {
    new_test_ext().execute_with(|| {
        const ALICE_TO_DUST_BALANCE: Balance = EXISTENTIAL_DEPOSIT - VALUE_PER_GAS;

        const BALANCE_DIFF: Balance = ALICE_BALANCE - ALICE_TO_DUST_BALANCE;
        const GAS_AMOUNT: u64 = (BALANCE_DIFF / VALUE_PER_GAS) as u64;

        assert_eq!(gas_price(GAS_AMOUNT), BALANCE_DIFF);

        assert_ok!(GearBank::deposit_gas(&ALICE, GAS_AMOUNT, false));

        assert_bank_balance(GAS_AMOUNT, 0);

        assert_balance(&ALICE, 0);
        assert_gas_value(&ALICE, GAS_AMOUNT, 0);
    })
}

#[test]
fn deposit_gas_zero() {
    new_test_ext().execute_with(|| {
        let h = sp_io::storage::root(StateVersion::V1);

        assert_ok!(GearBank::deposit_gas(&ALICE, 0, false));

        assert_ok!(GearBank::deposit_gas(&Zero::zero(), 0, false));

        // No-op operation assertion.
        assert_eq!(
            h,
            sp_io::storage::root(StateVersion::V1),
            "storage has been mutated"
        );
    })
}

#[test]
fn deposit_gas_insufficient_balance() {
    new_test_ext().execute_with(|| {
        const GAS_AMOUNT: u64 = u64::MAX;

        assert!(gas_price(GAS_AMOUNT) > Balances::free_balance(ALICE));

        assert_noop!(
            GearBank::deposit_gas(&ALICE, GAS_AMOUNT, false),
            Error::<Test>::InsufficientBalance
        );
    })
}

#[test]
fn deposit_gas_insufficient_deposit() {
    // Unreachable case for Gear protocol.
    new_test_ext().execute_with(|| {
        const GAS_AMOUNT: u64 = 1;

        assert!(gas_price(GAS_AMOUNT) < CurrencyOf::<Test>::minimum_balance());

        assert_ok!(Balances::transfer_all(
            RuntimeOrigin::signed(GearBank::bank_address()),
            Zero::zero(),
            false,
        ));

        assert_noop!(
            GearBank::deposit_gas(&ALICE, GAS_AMOUNT, false),
            Error::<Test>::InsufficientDeposit
        );
    })
}

#[test]
fn withdraw_gas_different_users() {
    new_test_ext().execute_with(|| {
        const ALICE_GAS: u64 = 1_234_567;
        assert_ok!(GearBank::deposit_gas(&ALICE, ALICE_GAS, false));

        const BOB_GAS: u64 = 56_789;
        assert_ok!(GearBank::deposit_gas(&BOB, BOB_GAS, false));

        const ALICE_WITHDRAW: u64 = ALICE_GAS - 123_456;
        assert_ok!(GearBank::withdraw_gas(&ALICE, ALICE_WITHDRAW, mult()));

        assert_bank_balance(ALICE_GAS - ALICE_WITHDRAW + BOB_GAS, 0);

        assert_alice_dec(gas_price(ALICE_GAS - ALICE_WITHDRAW));
        assert_gas_value(&ALICE, ALICE_GAS - ALICE_WITHDRAW, 0);

        assert_bob_dec(gas_price(BOB_GAS));
        assert_gas_value(&BOB, BOB_GAS, 0);

        const BOB_WITHDRAW: u64 = BOB_GAS - 1_234;
        assert_ok!(GearBank::withdraw_gas(&BOB, BOB_WITHDRAW, mult()));

        assert_bank_balance(ALICE_GAS - ALICE_WITHDRAW + BOB_GAS - BOB_WITHDRAW, 0);

        assert_alice_dec(gas_price(ALICE_GAS - ALICE_WITHDRAW));
        assert_gas_value(&ALICE, ALICE_GAS - ALICE_WITHDRAW, 0);

        assert_bob_dec(gas_price(BOB_GAS - BOB_WITHDRAW));
        assert_gas_value(&BOB, BOB_GAS - BOB_WITHDRAW, 0);
    })
}

#[test]
fn withdraw_gas_single_user() {
    new_test_ext().execute_with(|| {
        const GAS_AMOUNT: u64 = 123_456;
        assert_ok!(GearBank::deposit_gas(&ALICE, GAS_AMOUNT, false));

        const WITHDRAW_1: u64 = GAS_AMOUNT - 23_456;
        assert_ok!(GearBank::withdraw_gas(&ALICE, WITHDRAW_1, mult()));

        assert_bank_balance(GAS_AMOUNT - WITHDRAW_1, 0);

        assert_alice_dec(gas_price(GAS_AMOUNT - WITHDRAW_1));
        assert_gas_value(&ALICE, GAS_AMOUNT - WITHDRAW_1, 0);

        const WITHDRAW_2: u64 = GAS_AMOUNT - WITHDRAW_1 - 10_000;
        assert_ok!(GearBank::withdraw_gas(&ALICE, WITHDRAW_2, mult()));

        assert_bank_balance(GAS_AMOUNT - WITHDRAW_1 - WITHDRAW_2, 0);

        assert_alice_dec(gas_price(GAS_AMOUNT - WITHDRAW_1 - WITHDRAW_2));
        assert_gas_value(&ALICE, GAS_AMOUNT - WITHDRAW_1 - WITHDRAW_2, 0);
    })
}

#[test]
fn withdraw_gas_all_balance() {
    new_test_ext().execute_with(|| {
        const GAS_AMOUNT: u64 = 123_456;
        assert_ok!(GearBank::deposit_gas(&ALICE, GAS_AMOUNT, false));

        assert_ok!(GearBank::withdraw_gas(&ALICE, GAS_AMOUNT, mult()));

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

        assert_eq!(gas_price(GAS_AMOUNT), BALANCE_DIFF);
        assert!(BALANCE_DIFF > CurrencyOf::<Test>::minimum_balance());

        assert_ok!(GearBank::deposit_gas(&ALICE, GAS_AMOUNT, false));
        assert_ok!(GearBank::withdraw_gas(&ALICE, GAS_AMOUNT, mult()));

        assert_bank_balance(0, 0);

        assert_alice_dec(ALICE_TO_DUST_BALANCE);
        assert_gas_value(&ALICE, 0, 0);
    })
}

#[test]
fn withdraw_gas_small_amount() {
    new_test_ext().execute_with(|| {
        const GAS_AMOUNT: u64 = ((EXISTENTIAL_DEPOSIT - VALUE_PER_GAS) / VALUE_PER_GAS) as u64;

        assert!(gas_price(GAS_AMOUNT) < CurrencyOf::<Test>::minimum_balance());

        assert_ok!(GearBank::deposit_gas(&ALICE, GAS_AMOUNT, false));

        assert_ok!(GearBank::withdraw_gas(&ALICE, GAS_AMOUNT, mult()));

        assert_bank_balance(0, 0);

        assert_alice_dec(0);
        assert_gas_value(&ALICE, 0, 0);
    })
}

#[test]
fn withdraw_gas_small_amount_user_account_deleted() {
    new_test_ext().execute_with(|| {
        const GAS_VALUE_AMOUNT: Balance = (EXISTENTIAL_DEPOSIT - 1) / VALUE_PER_GAS * VALUE_PER_GAS;
        assert!(GAS_VALUE_AMOUNT < CurrencyOf::<Test>::minimum_balance());

        const GAS_AMOUNT: u64 = (GAS_VALUE_AMOUNT / VALUE_PER_GAS) as u64;
        assert_eq!(gas_price(GAS_AMOUNT), GAS_VALUE_AMOUNT);

        assert_ok!(GearBank::deposit_gas(&ALICE, GAS_AMOUNT, false));

        assert_ok!(Balances::transfer_all(
            RuntimeOrigin::signed(ALICE),
            Zero::zero(),
            false,
        ));

        assert_ok!(GearBank::withdraw_gas(&ALICE, GAS_AMOUNT, mult()));

        assert_eq!(UnusedValue::<Test>::get(), GAS_VALUE_AMOUNT);
        assert_balance(
            &GearBank::bank_address(),
            EXISTENTIAL_DEPOSIT + GAS_VALUE_AMOUNT,
        );

        assert_bank_balance(0, 0);

        assert_balance(&ALICE, 0);
        assert_gas_value(&ALICE, 0, 0);
    })
}

#[test]
fn withdraw_gas_zero() {
    new_test_ext().execute_with(|| {
        let h = sp_io::storage::root(StateVersion::V1);

        assert_ok!(GearBank::withdraw_gas(&ALICE, 0, mult()));

        assert_ok!(GearBank::withdraw_gas(&Zero::zero(), 0, mult()));

        // No-op operation assertion.
        assert_eq!(
            h,
            sp_io::storage::root(StateVersion::V1),
            "storage has been mutated"
        );
    })
}

#[test]
fn withdraw_gas_insufficient_bank_balance() {
    // Unreachable case for Gear protocol.
    new_test_ext().execute_with(|| {
        const GAS_AMOUNT: u64 = 123_456;

        assert_ok!(GearBank::deposit_gas(&ALICE, GAS_AMOUNT, false));

        assert_ok!(Balances::transfer_all(
            RuntimeOrigin::signed(GearBank::bank_address()),
            Zero::zero(),
            false,
        ));

        assert_noop!(
            GearBank::withdraw_gas(&ALICE, GAS_AMOUNT, mult()),
            Error::<Test>::InsufficientBankBalance
        );
    })
}

#[test]
fn withdraw_gas_insufficient_gas_balance() {
    new_test_ext().execute_with(|| {
        const GAS_AMOUNT: u64 = 123_456;

        assert_ok!(GearBank::deposit_gas(&ALICE, GAS_AMOUNT, false));

        assert_noop!(
            GearBank::withdraw_gas(&ALICE, GAS_AMOUNT + 1, mult()),
            Error::<Test>::InsufficientGasBalance
        );

        assert_ok!(GearBank::deposit_gas(&BOB, GAS_AMOUNT, false));

        assert_noop!(
            GearBank::withdraw_gas(&ALICE, GAS_AMOUNT + 1, mult()),
            Error::<Test>::InsufficientGasBalance
        );
    })
}

#[test]
fn withdraw_gas_insufficient_inexistent_gas_balance() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            GearBank::withdraw_gas(&ALICE, 1, mult()),
            Error::<Test>::InsufficientGasBalance
        );

        assert_noop!(
            GearBank::withdraw_gas(&Zero::zero(), 1, mult()),
            Error::<Test>::InsufficientGasBalance
        );

        const GAS_AMOUNT: u64 = 123_456;

        assert_ok!(GearBank::deposit_gas(&BOB, GAS_AMOUNT, false));

        assert_noop!(
            GearBank::withdraw_gas(&ALICE, 1, mult()),
            Error::<Test>::InsufficientGasBalance
        );

        assert_noop!(
            GearBank::withdraw_gas(&Zero::zero(), 1, mult()),
            Error::<Test>::InsufficientGasBalance
        );
    })
}

#[test]
fn spend_gas_different_users() {
    new_test_ext().execute_with(|| {
        const ALICE_GAS: u64 = 1_234_567;
        assert_ok!(GearBank::deposit_gas(&ALICE, ALICE_GAS, false));

        const BOB_GAS: u64 = 56_789;
        assert_ok!(GearBank::deposit_gas(&BOB, BOB_GAS, false));

        const ALICE_BURN: u64 = ALICE_GAS - 123_456;
        assert_ok!(GearBank::spend_gas(&ALICE, ALICE_BURN, mult()));
        GearBank::on_finalize(1);

        assert_bank_balance(ALICE_GAS - ALICE_BURN + BOB_GAS, 0);

        assert_block_author_inc(gas_price(ALICE_BURN));

        assert_alice_dec(gas_price(ALICE_GAS));
        assert_gas_value(&ALICE, ALICE_GAS - ALICE_BURN, 0);

        assert_bob_dec(gas_price(BOB_GAS));
        assert_gas_value(&BOB, BOB_GAS, 0);

        const BOB_BURN: u64 = BOB_GAS - 1_234;
        assert_ok!(GearBank::spend_gas(&BOB, BOB_BURN, mult()));
        GearBank::on_finalize(1);

        assert_bank_balance(ALICE_GAS - ALICE_BURN + BOB_GAS - BOB_BURN, 0);

        assert_block_author_inc(gas_price(ALICE_BURN + BOB_BURN));

        assert_alice_dec(gas_price(ALICE_GAS));
        assert_gas_value(&ALICE, ALICE_GAS - ALICE_BURN, 0);

        assert_bob_dec(gas_price(BOB_GAS));
        assert_gas_value(&BOB, BOB_GAS - BOB_BURN, 0);
    })
}

#[test]
fn spend_gas_single_user() {
    new_test_ext().execute_with(|| {
        const GAS_AMOUNT: u64 = 123_456;
        assert_ok!(GearBank::deposit_gas(&ALICE, GAS_AMOUNT, false));

        const BURN_1: u64 = GAS_AMOUNT - 23_456;
        assert_ok!(GearBank::spend_gas(&ALICE, BURN_1, mult()));
        GearBank::on_finalize(1);

        assert_bank_balance(GAS_AMOUNT - BURN_1, 0);

        assert_block_author_inc(gas_price(BURN_1));

        assert_alice_dec(gas_price(GAS_AMOUNT));
        assert_gas_value(&ALICE, GAS_AMOUNT - BURN_1, 0);

        const BURN_2: u64 = GAS_AMOUNT - BURN_1 - 10_000;
        assert_ok!(GearBank::spend_gas(&ALICE, BURN_2, mult()));
        GearBank::on_finalize(1);

        assert_bank_balance(GAS_AMOUNT - BURN_1 - BURN_2, 0);

        assert_block_author_inc(gas_price(BURN_1 + BURN_2));

        assert_alice_dec(gas_price(GAS_AMOUNT));
        assert_gas_value(&ALICE, GAS_AMOUNT - BURN_1 - BURN_2, 0);
    })
}

#[test]
fn spend_gas_all_balance() {
    new_test_ext().execute_with(|| {
        const GAS_AMOUNT: u64 = 123_456;
        assert_ok!(GearBank::deposit_gas(&ALICE, GAS_AMOUNT, false));

        assert_ok!(GearBank::spend_gas(&ALICE, GAS_AMOUNT, mult()));
        GearBank::on_finalize(1);

        assert_bank_balance(0, 0);

        assert_block_author_inc(gas_price(GAS_AMOUNT));

        assert_alice_dec(gas_price(GAS_AMOUNT));
        assert_gas_value(&ALICE, 0, 0);
    })
}

#[test]
fn spend_gas_all_balance_validator_account_deleted() {
    new_test_ext().execute_with(|| {
        const GAS_AMOUNT: u64 = 123_456;

        assert!(gas_price(GAS_AMOUNT) >= CurrencyOf::<Test>::minimum_balance());

        assert_ok!(GearBank::deposit_gas(&ALICE, GAS_AMOUNT, false));

        assert_ok!(Balances::transfer_all(
            RuntimeOrigin::signed(BLOCK_AUTHOR),
            Zero::zero(),
            false,
        ));

        assert_ok!(GearBank::spend_gas(&ALICE, GAS_AMOUNT, mult()));
        GearBank::on_finalize(1);

        assert_bank_balance(0, 0);

        let block_author_share = Percent::one() - TreasuryGasFeeShare::get();
        assert_balance(&BLOCK_AUTHOR, block_author_share * gas_price(GAS_AMOUNT));

        assert_alice_dec(gas_price(GAS_AMOUNT));
        assert_gas_value(&ALICE, 0, 0);
    })
}

#[test]
fn spend_gas_small_amount() {
    new_test_ext().execute_with(|| {
        const GAS_AMOUNT: u64 = ((EXISTENTIAL_DEPOSIT - VALUE_PER_GAS) / VALUE_PER_GAS) as u64;

        assert!(gas_price(GAS_AMOUNT) < CurrencyOf::<Test>::minimum_balance());

        assert_ok!(GearBank::deposit_gas(&ALICE, GAS_AMOUNT, false));

        assert_ok!(GearBank::spend_gas(&ALICE, GAS_AMOUNT, mult()));
        GearBank::on_finalize(1);

        assert_bank_balance(0, 0);

        assert_block_author_inc(gas_price(GAS_AMOUNT));

        assert_alice_dec(gas_price(GAS_AMOUNT));
        assert_gas_value(&ALICE, 0, 0);
    })
}

#[test]
fn spend_gas_small_amount_validator_account_deleted() {
    new_test_ext().execute_with(|| {
        const GAS_VALUE_AMOUNT: Balance = (EXISTENTIAL_DEPOSIT - 1) / VALUE_PER_GAS * VALUE_PER_GAS;
        assert!(GAS_VALUE_AMOUNT < CurrencyOf::<Test>::minimum_balance());

        const GAS_AMOUNT: u64 = (GAS_VALUE_AMOUNT / VALUE_PER_GAS) as u64;
        assert_eq!(gas_price(GAS_AMOUNT), GAS_VALUE_AMOUNT);

        assert_ok!(GearBank::deposit_gas(&ALICE, GAS_AMOUNT, false));

        assert_ok!(Balances::transfer_all(
            RuntimeOrigin::signed(BLOCK_AUTHOR),
            Zero::zero(),
            false,
        ));

        assert_ok!(GearBank::spend_gas(&ALICE, GAS_AMOUNT, mult()));
        GearBank::on_finalize(1);

        let block_author_share = Percent::one() - TreasuryGasFeeShare::get();
        let unused_value_inc = block_author_share * GAS_VALUE_AMOUNT;

        assert_eq!(UnusedValue::<Test>::get(), unused_value_inc);
        assert_balance(
            &GearBank::bank_address(),
            EXISTENTIAL_DEPOSIT + unused_value_inc,
        );

        assert_bank_balance(0, 0);

        assert_balance(&BLOCK_AUTHOR, 0);

        assert_alice_dec(gas_price(GAS_AMOUNT));
        assert_gas_value(&ALICE, 0, 0);
    })
}

#[test]
fn spend_gas_zero() {
    new_test_ext().execute_with(|| {
        let _block_author = Authorship::author();

        let h = sp_io::storage::root(StateVersion::V1);

        assert_ok!(GearBank::spend_gas(&ALICE, 0, mult()));

        assert_ok!(GearBank::spend_gas(&Zero::zero(), 0, mult()));

        // No-op operation assertion.
        assert_eq!(
            h,
            sp_io::storage::root(StateVersion::V1),
            "storage has been mutated"
        );
    })
}

#[test]
fn spend_gas_insufficient_bank_balance() {
    // Unreachable case for Gear protocol.
    new_test_ext().execute_with(|| {
        let _block_author = Authorship::author();

        const GAS_AMOUNT: u64 = 123_456;
        let bank_address = GearBank::bank_address();

        assert_ok!(GearBank::deposit_gas(&ALICE, GAS_AMOUNT, false));

        assert_ok!(Balances::transfer_all(
            RuntimeOrigin::signed(bank_address),
            Zero::zero(),
            false,
        ));

        assert_balance(&bank_address, 0);

        assert_noop!(
            GearBank::spend_gas(&ALICE, GAS_AMOUNT, mult()),
            Error::<Test>::InsufficientBankBalance
        );
    })
}

#[test]
fn spend_gas_insufficient_gas_balance() {
    new_test_ext().execute_with(|| {
        let _block_author = Authorship::author();

        const GAS_AMOUNT: u64 = 123_456;

        assert_ok!(GearBank::deposit_gas(&ALICE, GAS_AMOUNT, false));

        assert_noop!(
            GearBank::spend_gas(&ALICE, GAS_AMOUNT + 1, mult()),
            Error::<Test>::InsufficientGasBalance
        );

        assert_ok!(GearBank::deposit_gas(&BOB, GAS_AMOUNT, false));

        assert_noop!(
            GearBank::spend_gas(&ALICE, GAS_AMOUNT + 1, mult()),
            Error::<Test>::InsufficientGasBalance
        );
    })
}

#[test]
fn spend_gas_insufficient_inexistent_gas_balance() {
    new_test_ext().execute_with(|| {
        let _block_author = Authorship::author();

        assert_noop!(
            GearBank::spend_gas(&ALICE, 1, mult()),
            Error::<Test>::InsufficientGasBalance
        );

        assert_noop!(
            GearBank::spend_gas(&Zero::zero(), 1, mult()),
            Error::<Test>::InsufficientGasBalance
        );

        const GAS_AMOUNT: u64 = 123_456;
        assert_ok!(GearBank::deposit_gas(&BOB, GAS_AMOUNT, false));

        assert_noop!(
            GearBank::spend_gas(&ALICE, 1, mult()),
            Error::<Test>::InsufficientGasBalance
        );

        assert_noop!(
            GearBank::spend_gas(&Zero::zero(), 1, mult()),
            Error::<Test>::InsufficientGasBalance
        );
    })
}

#[test]
fn deposit_value_different_users() {
    new_test_ext().execute_with(|| {
        const ALICE_VALUE: Balance = 1_234_567_000;
        assert_ok!(GearBank::deposit_value(&ALICE, ALICE_VALUE, false));

        assert_bank_balance(0, ALICE_VALUE);

        assert_alice_dec(ALICE_VALUE);
        assert_gas_value(&ALICE, 0, ALICE_VALUE);

        const BOB_VALUE: Balance = 56_789_000;
        assert_ok!(GearBank::deposit_value(&BOB, BOB_VALUE, false));

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
        assert_ok!(GearBank::deposit_value(&ALICE, VALUE_1, false));

        assert_bank_balance(0, VALUE_1);

        assert_alice_dec(VALUE_1);
        assert_gas_value(&ALICE, 0, VALUE_1);

        const VALUE_2: Balance = 67_890_000;
        assert_ok!(GearBank::deposit_value(&ALICE, VALUE_2, false));

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

        assert_ok!(GearBank::deposit_value(&ALICE, VALUE, false));

        assert_bank_balance(0, VALUE);

        assert_balance(&ALICE, 0);
        assert_gas_value(&ALICE, 0, VALUE);
    })
}

#[test]
fn deposit_value_zero() {
    new_test_ext().execute_with(|| {
        let h = sp_io::storage::root(StateVersion::V1);

        assert_ok!(GearBank::deposit_value(&ALICE, 0, false));

        assert_ok!(GearBank::deposit_value(&Zero::zero(), 0, false));

        // No-op operation assertion.
        assert_eq!(
            h,
            sp_io::storage::root(StateVersion::V1),
            "storage has been mutated"
        );
    })
}

#[test]
fn deposit_value_overflow() {
    new_test_ext().execute_with(|| {
        const VALUE: Balance = Balance::MAX;

        assert!(VALUE > Balances::free_balance(ALICE));

        assert_noop!(
            GearBank::deposit_value(&ALICE, VALUE, false),
            Error::<Test>::Overflow
        );
    })
}

#[test]
fn deposit_value_insufficient_balance() {
    new_test_ext().execute_with(|| {
        const VALUE: Balance = Balance::MAX / 2;

        assert!(VALUE > Balances::free_balance(ALICE));

        assert_noop!(
            GearBank::deposit_value(&ALICE, VALUE, false),
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
            RuntimeOrigin::signed(GearBank::bank_address()),
            Zero::zero(),
            false,
        ));

        assert_noop!(
            GearBank::deposit_value(&ALICE, VALUE, false),
            Error::<Test>::InsufficientDeposit
        );
    })
}

#[test]
fn withdraw_value_different_users() {
    new_test_ext().execute_with(|| {
        const ALICE_VALUE: Balance = 1_234_567_000;
        assert_ok!(GearBank::deposit_value(&ALICE, ALICE_VALUE, false));

        const BOB_VALUE: Balance = 56_789_000;
        assert_ok!(GearBank::deposit_value(&BOB, BOB_VALUE, false));

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
        assert_ok!(GearBank::deposit_value(&ALICE, VALUE, false));

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
        assert_ok!(GearBank::deposit_value(&ALICE, VALUE, false));

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

        assert_ok!(GearBank::deposit_value(&ALICE, VALUE, false));
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

        assert_ok!(GearBank::deposit_value(&ALICE, VALUE, false));

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

        assert_ok!(GearBank::deposit_value(&ALICE, VALUE, false));

        assert_ok!(Balances::transfer_all(
            RuntimeOrigin::signed(ALICE),
            Zero::zero(),
            false,
        ));

        assert_ok!(GearBank::withdraw_value(&ALICE, VALUE));

        assert_eq!(UnusedValue::<Test>::get(), VALUE);
        assert_balance(&GearBank::bank_address(), EXISTENTIAL_DEPOSIT + VALUE);

        assert_bank_balance(0, 0);

        assert_balance(&ALICE, 0);
        assert_gas_value(&ALICE, 0, 0);
    })
}

#[test]
fn withdraw_value_zero() {
    new_test_ext().execute_with(|| {
        let h = sp_io::storage::root(StateVersion::V1);

        assert_ok!(GearBank::withdraw_value(&ALICE, 0));

        assert_ok!(GearBank::withdraw_value(&Zero::zero(), 0));

        // No-op operation assertion.
        assert_eq!(
            h,
            sp_io::storage::root(StateVersion::V1),
            "storage has been mutated"
        );
    })
}

#[test]
fn withdraw_value_insufficient_bank_balance() {
    // Unreachable case for Gear protocol.
    new_test_ext().execute_with(|| {
        const VALUE: Balance = 123_456_000;

        assert_ok!(GearBank::deposit_value(&ALICE, VALUE, false));

        assert_ok!(Balances::transfer_all(
            RuntimeOrigin::signed(GearBank::bank_address()),
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

        assert_ok!(GearBank::deposit_value(&ALICE, VALUE, false));

        assert_noop!(
            GearBank::withdraw_value(&ALICE, VALUE + 1),
            Error::<Test>::InsufficientValueBalance
        );

        assert_ok!(GearBank::deposit_value(&BOB, VALUE, false));

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

        const VALUE: Balance = 123_456_000;

        assert_ok!(GearBank::deposit_value(&BOB, VALUE, false));

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

#[test]
fn transfer_value_different_users() {
    new_test_ext().execute_with(|| {
        const ALICE_VALUE: Balance = 1_234_567_000;
        assert_ok!(GearBank::deposit_value(&ALICE, ALICE_VALUE, false));

        const BOB_VALUE: Balance = 56_789_000;
        assert_ok!(GearBank::deposit_value(&BOB, BOB_VALUE, false));

        const ALICE_TRANSFER: Balance = ALICE_VALUE - 123_456_000;
        assert_ok!(GearBank::transfer_value(&ALICE, &CHARLIE, ALICE_TRANSFER));

        assert_bank_balance(0, ALICE_VALUE - ALICE_TRANSFER + BOB_VALUE);

        assert_charlie_inc(ALICE_TRANSFER);

        assert_alice_dec(ALICE_VALUE);
        assert_gas_value(&ALICE, 0, ALICE_VALUE - ALICE_TRANSFER);

        assert_bob_dec(BOB_VALUE);
        assert_gas_value(&BOB, 0, BOB_VALUE);

        const BOB_TRANSFER: Balance = BOB_VALUE - 1_234_000;
        assert_ok!(GearBank::transfer_value(&BOB, &CHARLIE, BOB_TRANSFER));

        assert_bank_balance(0, ALICE_VALUE - ALICE_TRANSFER + BOB_VALUE - BOB_TRANSFER);

        assert_charlie_inc(ALICE_TRANSFER + BOB_TRANSFER);

        assert_alice_dec(ALICE_VALUE);
        assert_gas_value(&ALICE, 0, ALICE_VALUE - ALICE_TRANSFER);

        assert_bob_dec(BOB_VALUE);
        assert_gas_value(&BOB, 0, BOB_VALUE - BOB_TRANSFER);
    })
}

#[test]
fn transfer_value_single_user() {
    new_test_ext().execute_with(|| {
        const VALUE: Balance = 123_456_000;
        assert_ok!(GearBank::deposit_value(&ALICE, VALUE, false));

        const TRANSFER_1: Balance = VALUE - 23_456_000;
        assert_ok!(GearBank::transfer_value(&ALICE, &CHARLIE, TRANSFER_1));

        assert_bank_balance(0, VALUE - TRANSFER_1);

        assert_charlie_inc(TRANSFER_1);

        assert_alice_dec(VALUE);
        assert_gas_value(&ALICE, 0, VALUE - TRANSFER_1);

        const TRANSFER_2: Balance = VALUE - TRANSFER_1 - 10_000_000;
        assert_ok!(GearBank::transfer_value(&ALICE, &EVE, TRANSFER_2));

        assert_bank_balance(0, VALUE - TRANSFER_1 - TRANSFER_2);

        assert_charlie_inc(TRANSFER_1);
        assert_eve_inc(TRANSFER_2);

        assert_alice_dec(VALUE);
        assert_gas_value(&ALICE, 0, VALUE - TRANSFER_1 - TRANSFER_2);
    })
}

#[test]
fn transfer_value_self() {
    new_test_ext().execute_with(|| {
        const VALUE: Balance = 123_456_000;
        assert_ok!(GearBank::deposit_value(&ALICE, VALUE, false));

        const TRANSFER_1: Balance = VALUE - 23_456_000;
        assert_ok!(GearBank::transfer_value(&ALICE, &ALICE, TRANSFER_1));

        assert_bank_balance(0, VALUE - TRANSFER_1);

        assert_alice_dec(VALUE - TRANSFER_1);
        assert_gas_value(&ALICE, 0, VALUE - TRANSFER_1);

        const TRANSFER_2: Balance = VALUE - TRANSFER_1 - 10_000_000;
        assert_ok!(GearBank::transfer_value(&ALICE, &ALICE, TRANSFER_2));

        assert_bank_balance(0, VALUE - TRANSFER_1 - TRANSFER_2);

        assert_alice_dec(VALUE - TRANSFER_1 - TRANSFER_2);
        assert_gas_value(&ALICE, 0, VALUE - TRANSFER_1 - TRANSFER_2);
    })
}

#[test]
fn transfer_balance_all_balance() {
    new_test_ext().execute_with(|| {
        const VALUE: Balance = 123_456_000;
        assert_ok!(GearBank::deposit_value(&ALICE, VALUE, false));

        assert_ok!(GearBank::transfer_value(&ALICE, &CHARLIE, VALUE));

        assert_bank_balance(0, 0);

        assert_charlie_inc(VALUE);

        assert_alice_dec(VALUE);
        assert_gas_value(&ALICE, 0, 0);
    })
}

#[test]
fn transfer_value_all_balance_destination_account_deleted() {
    new_test_ext().execute_with(|| {
        const VALUE: Balance = 123_456_000;

        assert_ok!(GearBank::deposit_value(&ALICE, VALUE, false));

        assert_ok!(Balances::transfer_all(
            RuntimeOrigin::signed(CHARLIE),
            Zero::zero(),
            false,
        ));

        assert_ok!(GearBank::transfer_value(&ALICE, &CHARLIE, VALUE));

        assert_bank_balance(0, 0);

        assert_balance(&CHARLIE, VALUE);

        assert_alice_dec(VALUE);
        assert_gas_value(&ALICE, 0, 0);
    })
}

#[test]
fn transfer_value_all_balance_self_account_deleted() {
    new_test_ext().execute_with(|| {
        const VALUE: Balance = 123_456_000;

        assert_ok!(GearBank::deposit_value(&ALICE, VALUE, false));

        assert_ok!(Balances::transfer_all(
            RuntimeOrigin::signed(ALICE),
            Zero::zero(),
            false,
        ));

        assert_ok!(GearBank::transfer_value(&ALICE, &ALICE, VALUE));

        assert_bank_balance(0, 0);

        assert_balance(&ALICE, VALUE);
        assert_gas_value(&ALICE, 0, 0);
    })
}

#[test]
fn transfer_value_small_amount() {
    new_test_ext().execute_with(|| {
        const VALUE: Balance = EXISTENTIAL_DEPOSIT - 1;

        assert_ok!(GearBank::deposit_value(&ALICE, VALUE, false));

        assert_ok!(GearBank::transfer_value(&ALICE, &CHARLIE, VALUE));

        assert_bank_balance(0, 0);

        assert_charlie_inc(VALUE);

        assert_alice_dec(VALUE);
        assert_gas_value(&ALICE, 0, 0);
    })
}

#[test]
fn transfer_value_small_amount_destination_account_deleted() {
    new_test_ext().execute_with(|| {
        const VALUE: Balance = EXISTENTIAL_DEPOSIT - 1;
        assert_ok!(GearBank::deposit_value(&ALICE, VALUE, false));

        assert_ok!(Balances::transfer_all(
            RuntimeOrigin::signed(CHARLIE),
            Zero::zero(),
            false,
        ));

        assert_ok!(GearBank::transfer_value(&ALICE, &CHARLIE, VALUE));

        assert_eq!(UnusedValue::<Test>::get(), VALUE);
        assert_balance(&GearBank::bank_address(), EXISTENTIAL_DEPOSIT + VALUE);

        assert_bank_balance(0, 0);

        assert_balance(&CHARLIE, 0);

        assert_alice_dec(VALUE);
        assert_gas_value(&ALICE, 0, 0);
    })
}

#[test]
fn transfer_value_small_amount_self_account_deleted() {
    new_test_ext().execute_with(|| {
        const VALUE: Balance = EXISTENTIAL_DEPOSIT - 1;
        assert_ok!(GearBank::deposit_value(&ALICE, VALUE, false));

        assert_ok!(Balances::transfer_all(
            RuntimeOrigin::signed(ALICE),
            Zero::zero(),
            false,
        ));

        assert_ok!(GearBank::transfer_value(&ALICE, &ALICE, VALUE));

        assert_eq!(UnusedValue::<Test>::get(), VALUE);
        assert_balance(&GearBank::bank_address(), EXISTENTIAL_DEPOSIT + VALUE);

        assert_bank_balance(0, 0);

        assert_balance(&ALICE, 0);
        assert_gas_value(&ALICE, 0, 0);
    })
}

#[test]
fn transfer_value_zero() {
    new_test_ext().execute_with(|| {
        let h = sp_io::storage::root(StateVersion::V1);

        assert_ok!(GearBank::transfer_value(&ALICE, &ALICE, 0));
        assert_ok!(GearBank::transfer_value(&ALICE, &CHARLIE, 0));
        assert_ok!(GearBank::transfer_value(&ALICE, &Zero::zero(), 0));

        assert_ok!(GearBank::transfer_value(&Zero::zero(), &CHARLIE, 0));
        assert_ok!(GearBank::transfer_value(&Zero::zero(), &Zero::zero(), 0));

        // No-op operation assertion.
        assert_eq!(
            h,
            sp_io::storage::root(StateVersion::V1),
            "storage has been mutated"
        );
    })
}

#[test]
fn transfer_value_insufficient_bank_balance() {
    // Unreachable case for Gear protocol.
    new_test_ext().execute_with(|| {
        const VALUE: Balance = 123_456_000;
        let bank_address = GearBank::bank_address();

        assert_ok!(GearBank::deposit_value(&ALICE, VALUE, false));

        assert_ok!(Balances::transfer_all(
            RuntimeOrigin::signed(bank_address),
            Zero::zero(),
            false,
        ));

        assert_balance(&bank_address, 0);

        assert_noop!(
            GearBank::transfer_value(&ALICE, &CHARLIE, VALUE),
            Error::<Test>::InsufficientBankBalance
        );

        assert_noop!(
            GearBank::transfer_value(&ALICE, &ALICE, VALUE),
            Error::<Test>::InsufficientBankBalance
        );
    })
}

#[test]
fn transfer_value_insufficient_value_balance() {
    new_test_ext().execute_with(|| {
        const VALUE: Balance = 123_456_000;

        assert_ok!(GearBank::deposit_value(&ALICE, VALUE, false));

        assert_noop!(
            GearBank::transfer_value(&ALICE, &CHARLIE, VALUE + 1),
            Error::<Test>::InsufficientValueBalance
        );

        assert_noop!(
            GearBank::transfer_value(&ALICE, &ALICE, VALUE + 1),
            Error::<Test>::InsufficientValueBalance
        );

        assert_ok!(GearBank::deposit_value(&BOB, VALUE, false));

        assert_noop!(
            GearBank::transfer_value(&ALICE, &CHARLIE, VALUE + 1),
            Error::<Test>::InsufficientValueBalance
        );

        assert_noop!(
            GearBank::transfer_value(&ALICE, &ALICE, VALUE + 1),
            Error::<Test>::InsufficientValueBalance
        );
    })
}

#[test]
fn transfer_value_insufficient_inexistent_value_balance() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            GearBank::transfer_value(&ALICE, &ALICE, 1),
            Error::<Test>::InsufficientValueBalance
        );

        assert_noop!(
            GearBank::transfer_value(&ALICE, &CHARLIE, 1),
            Error::<Test>::InsufficientValueBalance
        );

        assert_noop!(
            GearBank::transfer_value(&Zero::zero(), &Zero::zero(), 1),
            Error::<Test>::InsufficientValueBalance
        );

        assert_noop!(
            GearBank::transfer_value(&Zero::zero(), &CHARLIE, 1),
            Error::<Test>::InsufficientValueBalance
        );

        const VALUE: Balance = 123_456_000;

        assert_ok!(GearBank::deposit_value(&BOB, VALUE, false));

        assert_noop!(
            GearBank::transfer_value(&ALICE, &ALICE, 1),
            Error::<Test>::InsufficientValueBalance
        );

        assert_noop!(
            GearBank::transfer_value(&ALICE, &CHARLIE, 1),
            Error::<Test>::InsufficientValueBalance
        );

        assert_noop!(
            GearBank::transfer_value(&Zero::zero(), &Zero::zero(), 1),
            Error::<Test>::InsufficientValueBalance
        );

        assert_noop!(
            GearBank::transfer_value(&Zero::zero(), &CHARLIE, 1),
            Error::<Test>::InsufficientValueBalance
        );
    })
}

#[test]
fn empty_accounts_deleted() {
    new_test_ext().execute_with(|| {
        assert!(GearBank::account(ALICE).is_none());

        const GAS_AMOUNT: u64 = 123_456;

        assert_ok!(GearBank::deposit_gas(&ALICE, GAS_AMOUNT, false));
        assert!(GearBank::account(ALICE).is_some());

        assert_ok!(GearBank::withdraw_gas(&ALICE, GAS_AMOUNT, mult()));
        assert!(GearBank::account(ALICE).is_none());

        assert_ok!(GearBank::deposit_gas(&ALICE, GAS_AMOUNT, false));
        assert!(GearBank::account(ALICE).is_some());

        assert_ok!(GearBank::spend_gas(&ALICE, GAS_AMOUNT, mult()));
        assert!(GearBank::account(ALICE).is_none());
        GearBank::on_finalize(1);

        const VALUE: Balance = 123_456_000;

        assert_ok!(GearBank::deposit_value(&ALICE, VALUE, false));
        assert!(GearBank::account(ALICE).is_some());

        assert_ok!(GearBank::withdraw_value(&ALICE, VALUE));
        assert!(GearBank::account(ALICE).is_none());

        assert_ok!(GearBank::deposit_value(&ALICE, VALUE, false));
        assert!(GearBank::account(ALICE).is_some());

        assert_ok!(GearBank::transfer_value(&ALICE, &CHARLIE, VALUE));
        assert!(GearBank::account(ALICE).is_none());

        assert_ok!(GearBank::deposit_value(&ALICE, VALUE, false));
        assert!(GearBank::account(ALICE).is_some());

        assert_ok!(GearBank::transfer_value(&ALICE, &ALICE, VALUE));
        assert!(GearBank::account(ALICE).is_none());
    })
}

#[test]
fn empty_zero_accounts_deleted() {
    new_test_ext().execute_with(|| {
        assert!(GearBank::account(<AccountId as Zero>::zero()).is_none());

        assert_ok!(GearBank::deposit_gas(&Zero::zero(), 0, false));
        assert!(GearBank::account(<AccountId as Zero>::zero()).is_none());

        assert_ok!(GearBank::withdraw_gas(&Zero::zero(), 0, mult()));
        assert!(GearBank::account(<AccountId as Zero>::zero()).is_none());

        assert_ok!(GearBank::spend_gas(&Zero::zero(), 0, mult()));
        assert!(GearBank::account(<AccountId as Zero>::zero()).is_none());
        GearBank::on_finalize(1);

        assert_ok!(GearBank::deposit_value(&Zero::zero(), 0, false));
        assert!(GearBank::account(<AccountId as Zero>::zero()).is_none());

        assert_ok!(GearBank::withdraw_value(&Zero::zero(), 0));
        assert!(GearBank::account(<AccountId as Zero>::zero()).is_none());

        assert_ok!(GearBank::transfer_value(&Zero::zero(), &ALICE, 0));
        assert!(GearBank::account(<AccountId as Zero>::zero()).is_none());

        assert_ok!(GearBank::transfer_value(&Zero::zero(), &Zero::zero(), 0));
        assert!(GearBank::account(<AccountId as Zero>::zero()).is_none());
    })
}

#[test]
fn empty_composite_accounts_deleted() {
    new_test_ext().execute_with(|| {
        const GAS_AMOUNT: u64 = 123_456;
        assert_ok!(GearBank::deposit_gas(&ALICE, GAS_AMOUNT, false));

        assert_bank_balance(GAS_AMOUNT, 0);

        assert!(GearBank::account(ALICE).is_some());
        assert_alice_dec(gas_price(GAS_AMOUNT));
        assert_gas_value(&ALICE, GAS_AMOUNT, 0);

        const VALUE: Balance = 234_567_000;
        assert_ok!(GearBank::deposit_value(&ALICE, VALUE, false));

        assert_bank_balance(GAS_AMOUNT, VALUE);

        assert!(GearBank::account(ALICE).is_some());
        assert_alice_dec(gas_price(GAS_AMOUNT) + VALUE);
        assert_gas_value(&ALICE, GAS_AMOUNT, VALUE);

        const GAS_BURN: u64 = GAS_AMOUNT / 2;

        assert_ok!(GearBank::spend_gas(&ALICE, GAS_BURN, mult()));
        GearBank::on_finalize(1);

        assert_bank_balance(GAS_AMOUNT - GAS_BURN, VALUE);

        assert!(GearBank::account(ALICE).is_some());
        assert_alice_dec(gas_price(GAS_AMOUNT) + VALUE);
        assert_gas_value(&ALICE, GAS_AMOUNT - GAS_BURN, VALUE);

        assert_ok!(GearBank::withdraw_value(&ALICE, VALUE));

        assert_bank_balance(GAS_AMOUNT - GAS_BURN, 0);

        assert!(GearBank::account(ALICE).is_some());
        assert_alice_dec(gas_price(GAS_AMOUNT));
        assert_gas_value(&ALICE, GAS_AMOUNT - GAS_BURN, 0);

        assert_ok!(GearBank::withdraw_gas(
            &ALICE,
            GAS_AMOUNT - GAS_BURN,
            mult()
        ));

        assert_bank_balance(0, 0);

        assert!(GearBank::account(ALICE).is_none());
        assert_alice_dec(gas_price(GAS_BURN));
        assert_gas_value(&ALICE, 0, 0);
    })
}

#[test]
fn spend_gas_on_finalize_different_users() {
    new_test_ext().execute_with(|| {
        const ALICE_GAS: u64 = 1_234_567;
        assert_ok!(GearBank::deposit_gas(&ALICE, ALICE_GAS, false));

        const BOB_GAS: u64 = 56_789;
        assert_ok!(GearBank::deposit_gas(&BOB, BOB_GAS, false));

        assert_eq!(OnFinalizeValue::<Test>::get(), 0);

        const ALICE_BURN: u64 = ALICE_GAS - 123_456;
        assert_ok!(GearBank::spend_gas(&ALICE, ALICE_BURN, mult()));

        assert_bank_balance(ALICE_GAS - ALICE_BURN + BOB_GAS, 0);

        assert_block_author_inc(0);
        assert_eq!(OnFinalizeValue::<Test>::get(), gas_price(ALICE_BURN));

        assert_alice_dec(gas_price(ALICE_GAS));
        assert_gas_value(&ALICE, ALICE_GAS - ALICE_BURN, 0);

        assert_bob_dec(gas_price(BOB_GAS));
        assert_gas_value(&BOB, BOB_GAS, 0);

        const BOB_BURN: u64 = BOB_GAS - 1_234;
        assert_ok!(GearBank::spend_gas(&BOB, BOB_BURN, mult()));

        assert_bank_balance(ALICE_GAS - ALICE_BURN + BOB_GAS - BOB_BURN, 0);

        assert_block_author_inc(0);
        assert_eq!(
            OnFinalizeValue::<Test>::get(),
            gas_price(ALICE_BURN + BOB_BURN)
        );

        assert_alice_dec(gas_price(ALICE_GAS));
        assert_gas_value(&ALICE, ALICE_GAS - ALICE_BURN, 0);

        assert_bob_dec(gas_price(BOB_GAS));
        assert_gas_value(&BOB, BOB_GAS - BOB_BURN, 0);

        /* what happens at the end of block */
        GearBank::on_finalize(1);
        assert_eq!(OnFinalizeValue::<Test>::get(), 0);
        assert_block_author_inc(gas_price(ALICE_BURN + BOB_BURN));

        GearBank::on_initialize(2);
    })
}

#[test]
fn spend_gas_on_finalize_single_user() {
    new_test_ext().execute_with(|| {
        const GAS_AMOUNT: u64 = 123_456;
        assert_ok!(GearBank::deposit_gas(&ALICE, GAS_AMOUNT, false));

        const BURN_1: u64 = GAS_AMOUNT - 23_456;
        assert_ok!(GearBank::spend_gas(&ALICE, BURN_1, mult()));

        assert_bank_balance(GAS_AMOUNT - BURN_1, 0);

        assert_eq!(OnFinalizeValue::<Test>::get(), gas_price(BURN_1));
        assert_block_author_inc(0);

        assert_alice_dec(gas_price(GAS_AMOUNT));
        assert_gas_value(&ALICE, GAS_AMOUNT - BURN_1, 0);

        const BURN_2: u64 = GAS_AMOUNT - BURN_1 - 10_000;
        assert_ok!(GearBank::spend_gas(&ALICE, BURN_2, mult()));

        assert_bank_balance(GAS_AMOUNT - BURN_1 - BURN_2, 0);

        assert_eq!(OnFinalizeValue::<Test>::get(), gas_price(BURN_1 + BURN_2));
        assert_block_author_inc(0);

        assert_alice_dec(gas_price(GAS_AMOUNT));
        assert_gas_value(&ALICE, GAS_AMOUNT - BURN_1 - BURN_2, 0);

        /* what happens at the end of block */
        GearBank::on_finalize(1);
        assert_eq!(OnFinalizeValue::<Test>::get(), 0);
        assert_block_author_inc(gas_price(BURN_1 + BURN_2));

        GearBank::on_initialize(2);
    })
}

#[test]
fn bank_address_always_in_storage() {
    new_test_ext().execute_with(|| {
        let key = BankAddress::<Test>::storage_value_final_key();
        unhashed::get::<<Test as frame_system::Config>::AccountId>(&key)
            .expect("Bank address not found in storage");
    })
}

mod utils {
    use super::*;

    // For some reason `assert_noop!` doesnt work for the pallet fns.
    impl PartialEq for Error<Test> {
        fn eq(&self, other: &Self) -> bool {
            match self {
                Self::InsufficientBalance => matches!(other, Self::InsufficientBalance),
                Self::InsufficientBankBalance => matches!(other, Self::InsufficientBankBalance),
                Self::InsufficientGasBalance => matches!(other, Self::InsufficientGasBalance),
                Self::InsufficientValueBalance => matches!(other, Self::InsufficientValueBalance),
                Self::InsufficientDeposit => matches!(other, Self::InsufficientDeposit),
                Self::Overflow => matches!(other, Self::Overflow),
                _ => unimplemented!(),
            }
        }
    }

    #[test]
    fn __existential_deposit() {
        new_test_ext().execute_with(|| {
            assert_eq!(EXISTENTIAL_DEPOSIT, CurrencyOf::<Test>::minimum_balance());
        })
    }

    pub fn mult() -> GasMultiplier<Test> {
        GasMultiplierOf::<Test>::get()
    }

    #[track_caller]
    pub fn assert_balance(account_id: &AccountId, value: Balance) {
        assert_eq!(Balances::total_balance(account_id), value);
        assert!(Balances::reserved_balance(account_id).is_zero());
    }

    #[track_caller]
    pub fn assert_bank_balance(gas: u64, value: Balance) {
        let gas_value = gas_price(gas);
        assert_balance(
            &GearBank::bank_address(),
            CurrencyOf::<Test>::minimum_balance()
                + UnusedValue::<Test>::get()
                + OnFinalizeValue::<Test>::get()
                + gas_value
                + value,
        );
    }

    #[track_caller]
    pub fn assert_gas_value(account_id: &AccountId, expected_gas: u64, expected_value: Balance) {
        let BankAccount { gas, value } = GearBank::account(account_id).unwrap_or_default();

        assert_eq!(gas, gas_price(expected_gas));
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
        let treasury_share = TreasuryGasFeeShare::get();
        assert_balance(&TREASURY, EXISTENTIAL_DEPOSIT + treasury_share * diff);

        let author_share = Percent::one() - treasury_share;
        assert_balance(&BLOCK_AUTHOR, EXISTENTIAL_DEPOSIT + author_share * diff)
    }

    // Asserts Charlie balance inc.
    #[track_caller]
    pub fn assert_charlie_inc(diff: Balance) {
        assert_balance(&CHARLIE, EXISTENTIAL_DEPOSIT + diff)
    }

    // Asserts Eve balance inc.
    #[track_caller]
    pub fn assert_eve_inc(diff: Balance) {
        assert_balance(&EVE, EXISTENTIAL_DEPOSIT + diff)
    }

    pub fn gas_price(gas: u64) -> u128 {
        <Test as crate::Config>::GasMultiplier::get().gas_to_value(gas)
    }
}
