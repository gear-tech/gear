// This file is part of Substrate.

// Copyright (C) 2020-2022 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Staking rewards pallet tests.

#![cfg(test)]

use crate::{mock::*, *};
use frame_support::{assert_noop, assert_ok};
use sp_runtime::{PerThing, Perbill};

macro_rules! assert_approx_eq {
    ($left:expr, $right:expr, $tol:expr) => {{
        assert!(
            $left <= $right + $tol && $right <= $left + $tol,
            "{} != {} with tolerance {}",
            $left,
            $right,
            $tol
        );
    }};
}

pub(crate) fn init_logger() {
    let _ = env_logger::Builder::from_default_env()
        .format_module_path(false)
        .format_level(true)
        .try_init();
}

#[test]
fn genesis_config_works() {
    init_logger();
    ExtBuilder::default()
        .initial_authorities(vec![
            (VAL_1_STASH, VAL_1_CONTROLLER, VAL_1_AUTH_ID),
            (VAL_2_STASH, VAL_2_CONTROLLER, VAL_2_AUTH_ID),
            (VAL_3_STASH, VAL_3_CONTROLLER, VAL_3_AUTH_ID),
        ])
        .stash(VALIDATOR_STAKE)
        .endowment(ENDOWMENT)
        .endowed_accounts(vec![SIGNER])
        .total_supply(INITIAL_TOTAL_TOKEN_SUPPLY)
        .non_stakeable(Perquintill::from_rational(4108_u64, 10_000_u64))
        .pool_balance(Perquintill::from_percent(11) * INITIAL_TOTAL_TOKEN_SUPPLY)
        .ideal_stake(Perquintill::from_percent(85))
        .target_inflation(Perquintill::from_rational(578_u64, 10_000_u64))
        .build()
        .execute_with(|| {
            assert_eq!(StakingRewards::pool(), 110_000 * UNITS);
        });
}

#[test]
fn pool_refill_works() {
    default_test_ext().execute_with(|| {
        // The initial pool state: empty
        assert_eq!(StakingRewards::pool(), 0);
        assert_ok!(StakingRewards::refill(RuntimeOrigin::signed(SIGNER), 100));
        assert_eq!(StakingRewards::pool(), 100);
    });
}

#[test]
fn burning_works() {
    ExtBuilder::default()
        .initial_authorities(vec![
            (VAL_1_STASH, VAL_1_CONTROLLER, VAL_1_AUTH_ID),
            (VAL_2_STASH, VAL_2_CONTROLLER, VAL_2_AUTH_ID),
            (VAL_3_STASH, VAL_3_CONTROLLER, VAL_3_AUTH_ID),
        ])
        .stash(VALIDATOR_STAKE)
        .endowment(ENDOWMENT)
        .endowed_accounts(vec![SIGNER, ROOT])
        .root(ROOT)
        .build()
        .execute_with(|| {
            Balances::make_free_balance_be(&StakingRewards::account_id(), 110);
            assert_eq!(StakingRewards::pool(), 100);
            assert_noop!(
                StakingRewards::withdraw(RuntimeOrigin::root(), SIGNER, 200),
                Error::<Test>::FailureToWithdrawFromPool
            );
            assert_ok!(StakingRewards::withdraw(RuntimeOrigin::root(), SIGNER, 50));
            assert_eq!(StakingRewards::pool(), 50);
        });
}

#[test]
fn rewards_account_doesnt_get_deleted() {
    ExtBuilder::default()
        .initial_authorities(vec![
            (VAL_1_STASH, VAL_1_CONTROLLER, VAL_1_AUTH_ID),
            (VAL_2_STASH, VAL_2_CONTROLLER, VAL_2_AUTH_ID),
            (VAL_3_STASH, VAL_3_CONTROLLER, VAL_3_AUTH_ID),
        ])
        .stash(VALIDATOR_STAKE)
        .endowment(ENDOWMENT)
        .endowed_accounts(vec![SIGNER, ROOT])
        .root(ROOT)
        .build()
        .execute_with(|| {
            Balances::make_free_balance_be(&StakingRewards::account_id(), 110);
            assert_eq!(StakingRewards::pool(), 100);
            assert_ok!(StakingRewards::withdraw(RuntimeOrigin::root(), SIGNER, 100));
            assert_eq!(StakingRewards::pool(), 0);
        });
}

#[test]
fn validators_rewards_disbursement_works() {
    let target_inflation = Perquintill::from_rational(578_u64, 10_000_u64);
    let ideal_stake = Perquintill::from_percent(85);
    let pool_balance = Perquintill::from_percent(14) * INITIAL_TOTAL_TOKEN_SUPPLY;
    let non_stakeable = Perquintill::from_perthousand(435);

    let mut ext = ExtBuilder::default()
        .initial_authorities(vec![
            (VAL_1_STASH, VAL_1_CONTROLLER, VAL_1_AUTH_ID),
            (VAL_2_STASH, VAL_2_CONTROLLER, VAL_2_AUTH_ID),
            (VAL_3_STASH, VAL_3_CONTROLLER, VAL_3_AUTH_ID),
        ])
        .stash(VALIDATOR_STAKE)
        .endowment(ENDOWMENT)
        .endowed_accounts(vec![SIGNER])
        .total_supply(INITIAL_TOTAL_TOKEN_SUPPLY)
        .non_stakeable(non_stakeable)
        .pool_balance(pool_balance)
        .ideal_stake(ideal_stake)
        .target_inflation(target_inflation)
        .build();
    ext.execute_with(|| {
        // Getting up-to-date data on era duration (they may differ from runtime constants)
        let sessions_per_era = <Test as pallet_staking::Config>::SessionsPerEra::get() as u64;
        let epoch_duration = SESSION_DURATION;
        let era_duration = sessions_per_era * epoch_duration;

        let num_validators = 3_u32;

        // Initial chain state
        let (
            initial_total_issuance,
            initial_total_stakeable,
            initial_treasury_balance,
            initial_rewards_pool_balance,
        ) = chain_state();

        assert_eq!(initial_total_issuance, INITIAL_TOTAL_TOKEN_SUPPLY);
        assert_eq!(initial_rewards_pool_balance, pool_balance);
        assert_eq!(
            initial_total_stakeable,
            (non_stakeable.left_from_one() * INITIAL_TOTAL_TOKEN_SUPPLY)
                .saturating_sub(pool_balance)
        );

        let era_duration_in_millis = era_duration * MILLISECS_PER_BLOCK;
        let era_time_fraction =
            Perquintill::from_rational(era_duration_in_millis, MILLISECONDS_PER_YEAR);

        // Running chain until era rollover
        run_to_block(era_duration + 1);

        // We don't check the correctness of inflation calculation as it has been verified
        let (expected_payout_0, expected_remainder) = compute_total_payout(
            0,
            initial_total_stakeable,
            initial_total_issuance,
            ideal_stake,
            <Test as Config>::MinInflation::get(),
            target_inflation,
            <Test as Config>::Falloff::get(),
            <Test as Config>::MaxROI::get(),
            era_time_fraction,
        );

        // Take up-to-date measurements of the chain stats
        let (total_issuance, total_stakeable, treasury_balance, rewards_pool_balance) =
            chain_state();

        // Total issuance shouldn't have changed
        assert_eq!(total_issuance, initial_total_issuance);
        // Treasury has been replenished
        assert_eq!(
            treasury_balance,
            initial_treasury_balance + expected_remainder
        );
        // The rewards pool has been used to offset newly minted currency
        assert_eq!(
            rewards_pool_balance,
            initial_rewards_pool_balance - (expected_payout_0 + expected_remainder)
        );
        // The total stakeable amount has grown accordingly
        assert_eq!(
            total_stakeable,
            initial_total_stakeable + expected_payout_0 + expected_remainder
        );

        // Validators reward stored in pallet_staking storage
        let validators_reward_for_era = Staking::eras_validator_reward(0)
            .expect("ErasValidatorReward storage must exist after era end; qed");
        assert_eq!(validators_reward_for_era, expected_payout_0);

        // Running chain until the next era rollover
        // Record the current state parameters
        let initial_total_issuance = total_issuance;
        let initial_total_stakeable = total_stakeable;
        let initial_treasury_balance = treasury_balance;
        let initial_rewards_pool_balance = rewards_pool_balance;

        run_to_block(2 * era_duration + 1);

        let total_staked = num_validators as u128 * VALIDATOR_STAKE;
        assert_eq!(total_staked, Staking::eras_total_stake(1));

        let (expected_payout_1, expected_remainder) = compute_total_payout(
            total_staked,
            initial_total_stakeable,
            initial_total_issuance,
            ideal_stake,
            <Test as Config>::MinInflation::get(),
            target_inflation,
            <Test as Config>::Falloff::get(),
            <Test as Config>::MaxROI::get(),
            era_time_fraction,
        );

        // Validators reward stored in pallet_staking storage
        let validators_reward_for_era = Staking::eras_validator_reward(1)
            .expect("ErasValidatorReward storage must exist after era end; qed");
        assert_eq!(validators_reward_for_era, expected_payout_1);

        // Trigger validators payout for the first 2 eras
        for era in 0_u32..2 {
            pallet_staking::Validators::<Test>::iter().for_each(|(stash_id, _)| {
                assert_ok!(Staking::payout_stakers(
                    RuntimeOrigin::signed(SIGNER),
                    stash_id,
                    era,
                ));
            });
        }

        // Update chain state parameters
        let (total_issuance, total_stakeable, treasury_balance, rewards_pool_balance) =
            chain_state();

        // Total issuance shouldn't have changed, again
        assert_eq!(total_issuance, initial_total_issuance);
        // Treasury has potentially been replenished
        assert_eq!(
            treasury_balance,
            initial_treasury_balance + expected_remainder
        );
        // The rewards pool has been used to offset newly minted currency
        assert_eq!(
            rewards_pool_balance,
            initial_rewards_pool_balance - (expected_payout_1 + expected_remainder)
        );
        // The total stakeable amount has grown accordingly
        assert_eq!(
            total_stakeable,
            initial_total_stakeable + expected_payout_1 + expected_remainder
        );

        // All the rewards should have landed at the VAL_1_STASH account -
        // the only one who has earned any points for authoring blocks
        assert_eq!(
            Balances::free_balance(VAL_1_STASH),
            VALIDATOR_STAKE + expected_payout_0 + expected_payout_1
        );
        // Other validators' balances remained intact
        assert_eq!(
            validators_total_balance(),
            VALIDATOR_STAKE * num_validators as u128 + expected_payout_0 + expected_payout_1
        );
    });
}

#[test]
fn nominators_rewards_disbursement_works() {
    let target_inflation = Perquintill::from_rational(578_u64, 10_000_u64);
    let ideal_stake = Perquintill::from_percent(85);
    let pool_balance = Perquintill::from_percent(14) * INITIAL_TOTAL_TOKEN_SUPPLY;
    let non_stakeable = Perquintill::from_perthousand(435);

    let mut ext = ExtBuilder::default()
        .initial_authorities(vec![
            (VAL_1_STASH, VAL_1_CONTROLLER, VAL_1_AUTH_ID),
            (VAL_2_STASH, VAL_2_CONTROLLER, VAL_2_AUTH_ID),
            (VAL_3_STASH, VAL_3_CONTROLLER, VAL_3_AUTH_ID),
        ])
        .stash(VALIDATOR_STAKE)
        .endowment(ENDOWMENT)
        .endowed_accounts(vec![SIGNER, NOM_1_STASH, NOM_1_CONTROLLER])
        .total_supply(INITIAL_TOTAL_TOKEN_SUPPLY)
        .non_stakeable(non_stakeable)
        .pool_balance(pool_balance)
        .ideal_stake(ideal_stake)
        .target_inflation(target_inflation)
        .build();
    ext.execute_with(|| {
        // Getting up-to-date data on era duration (they may differ from runtime constants)
        let sessions_per_era = <Test as pallet_staking::Config>::SessionsPerEra::get() as u64;
        let epoch_duration = SESSION_DURATION;
        let era_duration = sessions_per_era * epoch_duration;

        let num_validators = 3_u32;
        let num_nominators = 1_u32;

        let (
            initial_total_issuance,
            initial_total_stakeable,
            initial_treasury_balance,
            initial_rewards_pool_balance,
        ) = chain_state();
        let signer_balance = Balances::free_balance(SIGNER);
        let initial_validators_balance = validators_total_balance();
        assert_eq!(
            initial_total_issuance,
            initial_validators_balance
                + initial_treasury_balance
                + signer_balance
                + initial_rewards_pool_balance
                + 2 * ENDOWMENT // NOM_1_STASH and NOM_1_CONTROLLER
                + EXISTENTIAL_DEPOSIT // added to the rewards pool to ensure pool existence
        );
        assert_eq!(initial_rewards_pool_balance, pool_balance);
        assert_eq!(
            initial_total_stakeable,
            (non_stakeable.left_from_one() * initial_total_issuance).saturating_sub(pool_balance)
        );

        let era_duration_in_millis = era_duration * MILLISECS_PER_BLOCK;
        let era_time_fraction =
            Perquintill::from_rational(era_duration_in_millis, MILLISECONDS_PER_YEAR);

        // Bond and nominate
        run_to_block(10);

        // Send some funds to the nominator
        assert_ok!(Balances::transfer(
            RuntimeOrigin::signed(SIGNER),
            NOM_1_STASH,
            VALIDATOR_STAKE * 5, // 500 UNITS
        ));

        run_to_block(20);

        // Sending bonding transaction
        assert_ok!(Staking::bond(
            RuntimeOrigin::signed(NOM_1_STASH),
            NOM_1_CONTROLLER,
            VALIDATOR_STAKE * 5,
            pallet_staking::RewardDestination::Stash
        ));

        run_to_block(30);

        assert_ok!(Staking::nominate(
            RuntimeOrigin::signed(NOM_1_CONTROLLER),
            vec![VAL_1_STASH], // nominating "the best" validator
        ));
        let initial_nominators_balance = nominators_total_balance();

        // Running chain until era rollover
        run_to_block(era_duration + 1);

        // We don't check the correctness of inflation calculation as it has been verified
        let (expected_payout_0, expected_remainder) = compute_total_payout(
            0,
            initial_total_stakeable,
            initial_total_issuance,
            ideal_stake,
            <Test as Config>::MinInflation::get(),
            target_inflation,
            <Test as Config>::Falloff::get(),
            <Test as Config>::MaxROI::get(),
            era_time_fraction,
        );

        // Take up-to-date measurements of the chain stats
        let (total_issuance, total_stakeable, treasury_balance, rewards_pool_balance) =
            chain_state();

        // Total issuance shouldn't have changed
        assert_eq!(total_issuance, initial_total_issuance);
        // Treasury has been replenished
        assert_eq!(
            treasury_balance,
            initial_treasury_balance + expected_remainder
        );
        // The rewards pool has been used to offset newly minted currency
        assert_eq!(
            rewards_pool_balance,
            initial_rewards_pool_balance - (expected_payout_0 + expected_remainder)
        );
        // The total stakeable amount has grown accordingly
        assert_eq!(
            total_stakeable,
            initial_total_stakeable + expected_payout_0 + expected_remainder
        );

        // Validators reward stored in pallet_staking storage
        let validators_reward_for_era = Staking::eras_validator_reward(0)
            .expect("ErasValidatorReward storage must exist after era end; qed");
        assert_eq!(validators_reward_for_era, expected_payout_0);

        // Nominators total balance of stash accounts is still the same as before
        assert_eq!(nominators_total_balance(), initial_nominators_balance);

        // Running chain until the next era rollover
        // Record the current state parameters
        let initial_total_issuance = total_issuance;
        let initial_treasury_balance = treasury_balance;
        let initial_total_stakeable = total_stakeable;
        let initial_rewards_pool_balance = rewards_pool_balance;

        run_to_block(2 * era_duration + 1);

        // Staked amount now consists of validators and nominators' stake
        let validators_own_stake = num_validators as u128 * VALIDATOR_STAKE;
        let validators_other_stake = num_nominators as u128 * 5 * VALIDATOR_STAKE;
        let total_staked = validators_own_stake + validators_other_stake;
        assert_eq!(total_staked, Staking::eras_total_stake(1));

        let (expected_payout_1, expected_remainder) = compute_total_payout(
            total_staked,
            initial_total_stakeable,
            initial_total_issuance,
            ideal_stake,
            <Test as Config>::MinInflation::get(),
            target_inflation,
            <Test as Config>::Falloff::get(),
            <Test as Config>::MaxROI::get(),
            era_time_fraction,
        );

        // Validators reward stored in pallet_staking storage
        let validators_reward_for_era = Staking::eras_validator_reward(1)
            .expect("ErasValidatorReward storage must exist after era end; qed");
        assert_eq!(validators_reward_for_era, expected_payout_1);

        // Trigger validators payout for the first 2 eras
        for era in 0_u32..2 {
            pallet_staking::Validators::<Test>::iter().for_each(|(stash_id, _)| {
                assert_ok!(Staking::payout_stakers(
                    RuntimeOrigin::signed(SIGNER),
                    stash_id,
                    era,
                ));
            });
        }

        // Update chain state parameters
        let (total_issuance, total_stakeable, treasury_balance, rewards_pool_balance) =
            chain_state();

        // Total issuance shouldn't have changed, again
        assert_eq!(total_issuance, initial_total_issuance);
        // Treasury has potentially been replenished
        assert_eq!(
            treasury_balance,
            initial_treasury_balance + expected_remainder
        );
        // The rewards pool has been used to offset newly minted currency
        assert_eq!(
            rewards_pool_balance,
            initial_rewards_pool_balance - (expected_payout_1 + expected_remainder)
        );
        // The total stakeable amount has grown accordingly
        assert_eq!(
            total_stakeable,
            initial_total_stakeable + expected_payout_1 + expected_remainder
        );

        let nominators_balance_diff =
            nominators_total_balance().saturating_sub(initial_nominators_balance);
        let validators_balance_diff =
            validators_total_balance().saturating_sub(initial_validators_balance);

        // All points have been earn by the `VAL_1_STASH` with own to total stake ratio 1:6
        let nominators_fraction = Perbill::from_rational(5_u64, 6_u64);
        let expected_nominators_payout = nominators_fraction * expected_payout_1;
        assert_approx_eq!(nominators_balance_diff, expected_nominators_payout, 10);

        let expected_validators_payout = nominators_fraction.left_from_one() * expected_payout_1;
        assert_approx_eq!(validators_balance_diff, expected_validators_payout, 10);

        // All the rewards should have landed at the VAL_1_STASH account -
        // the only one who has earned any points for authoring blocks
        assert_eq!(
            Balances::free_balance(VAL_1_STASH),
            VALIDATOR_STAKE + expected_payout_0 + expected_validators_payout
        );
        // Other validators' balances remained intact
        assert_eq!(
            validators_total_balance(),
            VALIDATOR_STAKE * 3_u128 + expected_payout_0 + expected_validators_payout
        );
    });
}

#[test]
fn staking_blacklist_works() {
    use sp_runtime::{testing::TestXt, transaction_validity::InvalidTransaction};

    // let call = RuntimeCall::Staking(pallet_staking::Call::bond {
    //     controller: NOM_1_CONTROLLER,
    //     value: 10_000_u128,
    //     payee: pallet_staking::RewardDestination::Stash,
    // });

    let extra: SignedExtra = (
        StakingBlackList::<Test>::new(),
        frame_system::CheckNonce::<Test>::from(0),
        frame_system::CheckWeight::<Test>::new(),
    );

    let invalid = TestXt::<RuntimeCall, SignedExtra>::new(
        RuntimeCall::Staking(pallet_staking::Call::bond {
            controller: NOM_1_CONTROLLER,
            value: 10_000_u128,
            payee: pallet_staking::RewardDestination::Stash,
        }),
        Some((NOM_1_STASH, extra.clone())),
    );

    let valid_call = TestXt::<RuntimeCall, SignedExtra>::new(
        RuntimeCall::Balances(pallet_balances::Call::transfer {
            dest: NOM_1_CONTROLLER,
            value: 10_000_u128,
        }),
        Some((NOM_1_STASH, extra.clone())),
    );

    let valid_signer = TestXt::<RuntimeCall, SignedExtra>::new(
        RuntimeCall::Staking(pallet_staking::Call::bond {
            controller: NOM_1_CONTROLLER,
            value: 10_000_u128,
            payee: pallet_staking::RewardDestination::Stash,
        }),
        Some((SIGNER, extra)),
    );

    ExtBuilder::default()
        .initial_authorities(vec![
            (VAL_1_STASH, VAL_1_CONTROLLER, VAL_1_AUTH_ID),
            (VAL_2_STASH, VAL_2_CONTROLLER, VAL_2_AUTH_ID),
            (VAL_3_STASH, VAL_3_CONTROLLER, VAL_3_AUTH_ID),
        ])
        .stash(VALIDATOR_STAKE)
        .endowment(ENDOWMENT)
        .endowed_accounts(vec![SIGNER, NOM_1_STASH, NOM_1_CONTROLLER])
        .filtered_accounts(vec![NOM_1_STASH])
        .build()
        .execute_with(|| {
            assert_eq!(
                Executive::validate_transaction(
                    sp_runtime::transaction_validity::TransactionSource::External,
                    invalid,
                    Default::default(),
                )
                .unwrap_err(),
                InvalidTransaction::Call.into()
            );

            assert_ok!(Executive::validate_transaction(
                sp_runtime::transaction_validity::TransactionSource::External,
                valid_call,
                Default::default(),
            ));

            assert_ok!(Executive::validate_transaction(
                sp_runtime::transaction_validity::TransactionSource::External,
                valid_signer,
                Default::default(),
            ));
        });
}
