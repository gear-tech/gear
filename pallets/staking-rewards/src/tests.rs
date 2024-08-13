// This file is part of Gear.

// Copyright (C) 2021-2024 Gear Technologies Inc.
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

//! Staking rewards pallet tests.

use crate::{mock::*, *};
use frame_support::{assert_noop, assert_ok, assert_storage_noop, traits::EstimateNextNewSession};
use sp_runtime::{traits::Convert, DispatchError, PerThing, Perbill};

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
fn supply_alignment_works() {
    init_logger();

    ExtBuilder::<Test>::default()
        .initial_authorities(vec![
            (VAL_1_STASH, VAL_1_AUTH_ID),
            (VAL_2_STASH, VAL_2_AUTH_ID),
            (VAL_3_STASH, VAL_3_AUTH_ID),
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
            let assert_issuance =
                |balance: BalanceOf<Test>| assert_eq!(Balances::total_issuance(), balance);

            let assert_pool = |balance: BalanceOf<Test>| {
                assert_eq!(
                    Balances::total_balance(&StakingRewards::account_id()),
                    balance + Balances::minimum_balance()
                )
            };

            // Asserting initial parameters.
            assert_issuance(INITIAL_TOTAL_TOKEN_SUPPLY);

            let initial_pool_balance = Perquintill::from_percent(11) * INITIAL_TOTAL_TOKEN_SUPPLY;
            assert_pool(initial_pool_balance);

            // Asserting bad origin.
            assert_noop!(
                StakingRewards::align_supply(
                    RuntimeOrigin::signed(SIGNER),
                    INITIAL_TOTAL_TOKEN_SUPPLY
                ),
                DispatchError::BadOrigin
            );

            // Asserting no-op in case of equity.
            assert_storage_noop!(assert_ok!(StakingRewards::align_supply(
                RuntimeOrigin::root(),
                INITIAL_TOTAL_TOKEN_SUPPLY
            )));

            // Burning N tokens.
            let n = Balances::minimum_balance() * 5;

            assert_ok!(Balances::force_set_balance(
                RuntimeOrigin::root(),
                SIGNER,
                Balances::free_balance(SIGNER) - n,
            ));

            assert_issuance(INITIAL_TOTAL_TOKEN_SUPPLY - n);
            assert_pool(initial_pool_balance);

            // Aligning supply.
            assert_ok!(StakingRewards::align_supply(
                RuntimeOrigin::root(),
                INITIAL_TOTAL_TOKEN_SUPPLY
            ));

            System::assert_has_event(Event::Minted { amount: n }.into());
            assert_issuance(INITIAL_TOTAL_TOKEN_SUPPLY);
            assert_pool(initial_pool_balance + n);

            // Minting M tokens.
            let m = Balances::minimum_balance() * 12;

            assert_ok!(Balances::force_set_balance(
                RuntimeOrigin::root(),
                SIGNER,
                Balances::free_balance(SIGNER) + m,
            ));

            assert_issuance(INITIAL_TOTAL_TOKEN_SUPPLY + m);
            assert_pool(initial_pool_balance + n);

            // Aligning supply.
            assert_ok!(StakingRewards::align_supply(
                RuntimeOrigin::root(),
                INITIAL_TOTAL_TOKEN_SUPPLY
            ));

            System::assert_has_event(Event::Burned { amount: m }.into());
            assert_issuance(INITIAL_TOTAL_TOKEN_SUPPLY);
            assert_pool(initial_pool_balance + n - m);
        });
}

#[test]
fn genesis_config_works() {
    init_logger();
    ExtBuilder::<Test>::default()
        .initial_authorities(vec![
            (VAL_1_STASH, VAL_1_AUTH_ID),
            (VAL_2_STASH, VAL_2_AUTH_ID),
            (VAL_3_STASH, VAL_3_AUTH_ID),
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
    ExtBuilder::<Test>::default()
        .initial_authorities(vec![
            (VAL_1_STASH, VAL_1_AUTH_ID),
            (VAL_2_STASH, VAL_2_AUTH_ID),
            (VAL_3_STASH, VAL_3_AUTH_ID),
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
    ExtBuilder::<Test>::default()
        .initial_authorities(vec![
            (VAL_1_STASH, VAL_1_AUTH_ID),
            (VAL_2_STASH, VAL_2_AUTH_ID),
            (VAL_3_STASH, VAL_3_AUTH_ID),
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
    init_logger();

    let (target_inflation, ideal_stake, pool_balance, non_stakeable) = sensible_defaults();

    let mut ext = ExtBuilder::<Test>::default()
        .initial_authorities(vec![
            (VAL_1_STASH, VAL_1_AUTH_ID),
            (VAL_2_STASH, VAL_2_AUTH_ID),
            (VAL_3_STASH, VAL_3_AUTH_ID),
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
        let (expected_payout_0, _expected_remainder) = compute_total_payout(
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
        // Overriding the `expected_remainder` with 0 since the remainder should've been burned
        // TODO: remove the below when the `Treasury` part is no longer burnt
        let expected_remainder = 0;
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

        let (expected_payout_1, _expected_remainder) = compute_total_payout(
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
        // TODO: remove the below when the `Treasury` part is no longer burnt
        let expected_remainder = 0;
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
    init_logger();

    let (target_inflation, ideal_stake, pool_balance, non_stakeable) = sensible_defaults();
    let mut ext = with_parameters(target_inflation, ideal_stake, pool_balance, non_stakeable);
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
                // NOM_1_STASH
                + ENDOWMENT
                // added to the rewards and the rent pools to ensure pool existence
                + 2 * EXISTENTIAL_DEPOSIT
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
        assert_ok!(Balances::transfer_allow_death(
            RuntimeOrigin::signed(SIGNER),
            NOM_1_STASH,
            VALIDATOR_STAKE * 5, // 500 UNITS
        ));

        run_to_block(20);

        // Sending bonding transaction
        assert_ok!(Staking::bond(
            RuntimeOrigin::signed(NOM_1_STASH),
            VALIDATOR_STAKE * 5,
            pallet_staking::RewardDestination::Stash
        ));

        run_to_block(30);

        assert_ok!(Staking::nominate(
            RuntimeOrigin::signed(NOM_1_STASH),
            vec![VAL_1_STASH], // nominating "the best" validator
        ));
        let initial_nominators_balance = nominators_total_balance();

        // Running chain until era rollover
        run_to_block(era_duration + 1);

        // We don't check the correctness of inflation calculation as it has been verified
        let (expected_payout_0, _expected_remainder) = compute_total_payout(
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
        // TODO: remove the below when the `Treasury` part is no longer burnt
        let expected_remainder = 0;
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

        let (expected_payout_1, _expected_remainder) = compute_total_payout(
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
        // TODO: remove the below when the `Treasury` part is no longer burnt
        let expected_remainder = 0;
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

    init_logger();

    let extra: SignedExtra = StakingBlackList::<Test>::new();

    let invalid_call = TestXt::<RuntimeCall, SignedExtra>::new(
        RuntimeCall::Staking(pallet_staking::Call::bond {
            value: 10_000_u128,
            payee: pallet_staking::RewardDestination::Stash,
        }),
        Some((NOM_1_STASH, extra.clone())),
    );

    // Wrapping `bond` call in a batch is also illegal
    let invalid_batch = TestXt::<RuntimeCall, SignedExtra>::new(
        RuntimeCall::Utility(pallet_utility::Call::batch {
            calls: vec![RuntimeCall::Staking(pallet_staking::Call::bond {
                value: 10_000_u128,
                payee: pallet_staking::RewardDestination::Stash,
            })],
        }),
        Some((NOM_1_STASH, extra.clone())),
    );

    let invalid_batch_all = TestXt::<RuntimeCall, SignedExtra>::new(
        RuntimeCall::Utility(pallet_utility::Call::batch_all {
            calls: vec![RuntimeCall::Staking(pallet_staking::Call::bond {
                value: 10_000_u128,
                payee: pallet_staking::RewardDestination::Stash,
            })],
        }),
        Some((NOM_1_STASH, extra.clone())),
    );

    // Nested batches and/or other `Utility` calls shouldn't work, as well
    let nested_batches = TestXt::<RuntimeCall, SignedExtra>::new(
        RuntimeCall::Utility(pallet_utility::Call::batch {
            calls: vec![RuntimeCall::Utility(pallet_utility::Call::batch_all {
                calls: vec![RuntimeCall::Utility(pallet_utility::Call::as_derivative {
                    index: 0,
                    call: Box::new(RuntimeCall::Staking(pallet_staking::Call::bond {
                        value: 10_000_u128,
                        payee: pallet_staking::RewardDestination::Stash,
                    })),
                })],
            })],
        }),
        Some((NOM_1_STASH, extra.clone())),
    );

    let valid_call = TestXt::<RuntimeCall, SignedExtra>::new(
        RuntimeCall::Balances(pallet_balances::Call::transfer_allow_death {
            dest: NOM_1_STASH,
            value: 10_000_u128,
        }),
        Some((NOM_1_STASH, extra.clone())),
    );

    let valid_signer = TestXt::<RuntimeCall, SignedExtra>::new(
        RuntimeCall::Staking(pallet_staking::Call::bond {
            value: 10_000_u128,
            payee: pallet_staking::RewardDestination::Stash,
        }),
        Some((SIGNER, extra)),
    );

    ExtBuilder::<Test>::default()
        .initial_authorities(vec![
            (VAL_1_STASH, VAL_1_AUTH_ID),
            (VAL_2_STASH, VAL_2_AUTH_ID),
            (VAL_3_STASH, VAL_3_AUTH_ID),
        ])
        .stash(VALIDATOR_STAKE)
        .endowment(ENDOWMENT)
        .endowed_accounts(vec![SIGNER, NOM_1_STASH])
        .filtered_accounts(vec![NOM_1_STASH])
        .build()
        .execute_with(|| {
            assert_eq!(
                Executive::validate_transaction(
                    sp_runtime::transaction_validity::TransactionSource::External,
                    invalid_call,
                    Default::default(),
                )
                .unwrap_err(),
                InvalidTransaction::Call.into()
            );

            assert_eq!(
                Executive::validate_transaction(
                    sp_runtime::transaction_validity::TransactionSource::External,
                    invalid_batch,
                    Default::default(),
                )
                .unwrap_err(),
                InvalidTransaction::Call.into()
            );

            assert_eq!(
                Executive::validate_transaction(
                    sp_runtime::transaction_validity::TransactionSource::External,
                    invalid_batch_all,
                    Default::default(),
                )
                .unwrap_err(),
                InvalidTransaction::Call.into()
            );

            assert_eq!(
                Executive::validate_transaction(
                    sp_runtime::transaction_validity::TransactionSource::External,
                    nested_batches,
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

#[test]
fn inflation_at_ideal_staked_adds_up() {
    init_logger();

    let (target_inflation, ideal_stake, pool_balance, non_stakeable) = sensible_defaults();
    let mut ext = with_parameters(target_inflation, ideal_stake, pool_balance, non_stakeable);
    ext.execute_with(|| {
        // Getting up-to-date data on era duration (they may differ from runtime constants)
        let sessions_per_era = <Test as pallet_staking::Config>::SessionsPerEra::get() as u64;
        let epoch_duration = SESSION_DURATION;
        let era_duration = sessions_per_era * epoch_duration;

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
                // NOM_1_STASH
                + ENDOWMENT
                // added to the rewards and the rent pools to ensure pool existence
                + 2 * EXISTENTIAL_DEPOSIT
        );
        assert_eq!(initial_rewards_pool_balance, pool_balance);
        assert_eq!(
            initial_total_stakeable,
            (non_stakeable.left_from_one() * initial_total_issuance).saturating_sub(pool_balance)
        );

        let era_duration_in_millis = era_duration * MILLISECS_PER_BLOCK;

        // Bond and nominate
        run_to_block(10);

        let ideal_staked_value = ideal_stake * initial_total_stakeable;
        let nominator_stake = ideal_staked_value.saturating_sub(initial_validators_balance);

        // Send some funds to the nominator
        assert_ok!(Balances::transfer_allow_death(
            RuntimeOrigin::signed(SIGNER),
            NOM_1_STASH,
            nominator_stake,
        ));

        run_to_block(20);

        // Sending bonding transaction
        assert_ok!(Staking::bond(
            RuntimeOrigin::signed(NOM_1_STASH),
            nominator_stake,
            pallet_staking::RewardDestination::Stash
        ));

        run_to_block(30);

        assert_ok!(Staking::nominate(
            RuntimeOrigin::signed(NOM_1_STASH),
            vec![VAL_1_STASH], // nominating "the best" validator
        ));
        let initial_nominators_balance = nominators_total_balance();

        // Running chain until era rollover
        run_to_block(era_duration + 1);

        // No payout is expected for era #0 anyway because the "official" staked amount is 0
        assert_eq!(
            Staking::eras_validator_reward(0)
                .expect("ErasValidatorReward storage must exist after era end; qed"),
            0
        );

        // Test outline:
        // - running chain for almost `<T as pallet_staking::Config>::HistoryDepth` eras;
        // - not claiming any validators rewards in the meantime in order to preserve `stakeable`
        //   and `staked` amounts;
        // - at the end of 84 eras (2 weeks) period claim all rewards and sum up the validators'
        //   and the nominator's balances-in-excess - this would account for all minted funds.
        // - ensure the minted amount corresponds to the 5.78% p.a. inflation

        let history_depth = <Test as pallet_staking::Config>::HistoryDepth::get();

        // Running chain for 84 eras
        run_to_block(history_depth as u64 * era_duration + 1);
        // Claim rewards
        for era in 0_u32..history_depth {
            pallet_staking::Validators::<Test>::iter().for_each(|(stash_id, _)| {
                assert_ok!(Staking::payout_stakers(
                    RuntimeOrigin::signed(SIGNER),
                    stash_id,
                    era,
                ));
            });
        }

        // Take up-to-date measurements of the chain stats
        let (total_issuance, total_stakeable, _treasury_balance, rewards_pool_balance) =
            chain_state();

        // Total issuance shouldn't have changed
        assert_eq!(total_issuance, initial_total_issuance);
        // The rewards pool has been used to offset minted rewards
        let actual_rewards = initial_rewards_pool_balance.saturating_sub(rewards_pool_balance);
        let stakeable_delta = total_stakeable.saturating_sub(initial_total_stakeable);
        assert_eq!(actual_rewards, stakeable_delta);

        // Expected
        let overall_time_fraction = Perquintill::from_rational(
            history_depth.saturating_sub(1) as u64 * era_duration_in_millis,
            MILLISECONDS_PER_YEAR,
        );
        let annualized_rewards = target_inflation * initial_total_issuance;
        let expected_rewards = overall_time_fraction * annualized_rewards;
        // Rounding error could have accumulated over many eras
        assert_approx_eq!(
            actual_rewards,
            expected_rewards,
            actual_rewards / 10_000_000 // 0.00001%
        );

        let validators_balance_delta =
            validators_total_balance().saturating_sub(initial_validators_balance);
        let nominators_balance_delta =
            nominators_total_balance().saturating_sub(initial_nominators_balance);
        assert_eq!(
            validators_balance_delta + nominators_balance_delta,
            actual_rewards
        );
    });
}

#[test]
fn inflation_when_nobody_stakes_adds_up() {
    init_logger();

    let (target_inflation, ideal_stake, pool_balance, non_stakeable) = sensible_defaults();
    let mut ext = with_parameters(target_inflation, ideal_stake, pool_balance, non_stakeable);
    ext.execute_with(|| {
        // Getting up-to-date data on era duration (they may differ from runtime constants)
        let sessions_per_era = <Test as pallet_staking::Config>::SessionsPerEra::get() as u64;
        let epoch_duration = SESSION_DURATION;
        let era_duration = sessions_per_era * epoch_duration;

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
                // NOM_1_STASH
                + ENDOWMENT
                // added to the rewards and the rent pools to ensure pool existence
                + 2 * EXISTENTIAL_DEPOSIT
        );
        assert_eq!(initial_rewards_pool_balance, pool_balance);
        assert_eq!(
            initial_total_stakeable,
            (non_stakeable.left_from_one() * initial_total_issuance).saturating_sub(pool_balance)
        );

        let era_duration_in_millis = era_duration * MILLISECS_PER_BLOCK;

        // Bond and nominate
        run_to_block(10);

        let target_stake = Perquintill::from_percent(10);
        let target_staked_value = target_stake * initial_total_stakeable;
        let nominator_stake = target_staked_value.saturating_sub(initial_validators_balance);
        // Yearly inflation corresponding to 10% staking ratio is 1.5623529%
        let yearly_inflation = Perquintill::from_parts(15_623_529_411_764_700);

        // Send some funds to the nominator
        assert_ok!(Balances::transfer_allow_death(
            RuntimeOrigin::signed(SIGNER),
            NOM_1_STASH,
            nominator_stake,
        ));

        run_to_block(20);

        // Sending bonding transaction
        assert_ok!(Staking::bond(
            RuntimeOrigin::signed(NOM_1_STASH),
            nominator_stake,
            pallet_staking::RewardDestination::Stash
        ));

        run_to_block(30);

        assert_ok!(Staking::nominate(
            RuntimeOrigin::signed(NOM_1_STASH),
            vec![VAL_1_STASH], // nominating "the best" validator
        ));
        let initial_nominators_balance = nominators_total_balance();

        // Running chain until era rollover
        run_to_block(era_duration + 1);

        // No payout is expected for era #0 anyway because the "official" staked amount is 0
        assert_eq!(
            Staking::eras_validator_reward(0)
                .expect("ErasValidatorReward storage must exist after era end; qed"),
            0
        );

        // Test outline:
        // - running chain for almost `<T as pallet_staking::Config>::HistoryDepth` eras;
        // - not claiming any validators rewards in the meantime in order to preserve `stakeable`
        //   and `staked` amounts;
        // - at the end of 84 eras (2 weeks) period claim all rewards and sum up the validators'
        //   and the nominator's balances-in-excess - this would account for all minted funds.
        // - ensure the minted amount corresponds to the 1.5623529% p.a. inflation less the amount
        //   that exceeds the ROI cap of 30% (7.985% of minted amount is burned).

        let history_depth = <Test as pallet_staking::Config>::HistoryDepth::get();

        // Running chain for 84 eras
        run_to_block(history_depth as u64 * era_duration + 1);
        // Claim rewards
        for era in 0_u32..history_depth {
            pallet_staking::Validators::<Test>::iter().for_each(|(stash_id, _)| {
                assert_ok!(Staking::payout_stakers(
                    RuntimeOrigin::signed(SIGNER),
                    stash_id,
                    era,
                ));
            });
        }

        // Take up-to-date measurements of the chain stats
        let (total_issuance, total_stakeable, _treasury_balance, rewards_pool_balance) =
            chain_state();

        // Total issuance shouldn't have changed
        assert_eq!(total_issuance, initial_total_issuance);
        // The rewards pool has been used to offset minted rewards
        let actual_rewards = initial_rewards_pool_balance.saturating_sub(rewards_pool_balance);
        let stakeable_delta = total_stakeable.saturating_sub(initial_total_stakeable);
        assert_eq!(actual_rewards, stakeable_delta);

        // Expected
        let overall_time_fraction = Perquintill::from_rational(
            history_depth.saturating_sub(1) as u64 * era_duration_in_millis,
            MILLISECONDS_PER_YEAR,
        );
        let annualized_rewards = yearly_inflation * initial_total_issuance;
        let expected_rewards_raw = overall_time_fraction * annualized_rewards;

        // Given 10% staking rate, the respective ROI would exceed 30% cap
        // Therefore the part in excess must be burned (or sent to Treasury)
        let reward_ratio = Perquintill::from_rational(30_000_u64, 32_603_u64);
        let expected_rewards = reward_ratio * expected_rewards_raw;

        // Rounding error could have accumulated over many eras
        assert_approx_eq!(
            actual_rewards,
            expected_rewards,
            actual_rewards / 10_000 // 0.01%
        );

        let validators_balance_delta =
            validators_total_balance().saturating_sub(initial_validators_balance);
        let nominators_balance_delta =
            nominators_total_balance().saturating_sub(initial_nominators_balance);
        assert_eq!(
            validators_balance_delta + nominators_balance_delta,
            actual_rewards
        );
    });
}

#[test]
fn inflation_with_too_many_stakers_adds_up() {
    init_logger();

    let (target_inflation, ideal_stake, pool_balance, non_stakeable) = sensible_defaults();
    let mut ext = with_parameters(target_inflation, ideal_stake, pool_balance, non_stakeable);
    ext.execute_with(|| {
        // Getting up-to-date data on era duration (they may differ from runtime constants)
        let sessions_per_era = <Test as pallet_staking::Config>::SessionsPerEra::get() as u64;
        let epoch_duration = SESSION_DURATION;
        let era_duration = sessions_per_era * epoch_duration;

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
                // NOM_1_STASH
                + ENDOWMENT
                // added to the rewards and the rent pools to ensure pool existence
                + 2 * EXISTENTIAL_DEPOSIT
        );
        assert_eq!(initial_rewards_pool_balance, pool_balance);
        assert_eq!(
            initial_total_stakeable,
            (non_stakeable.left_from_one() * initial_total_issuance).saturating_sub(pool_balance)
        );

        let era_duration_in_millis = era_duration * MILLISECS_PER_BLOCK;

        // Bond and nominate
        run_to_block(10);

        let target_stake = Perquintill::from_percent(92);
        let target_staked_value = target_stake * initial_total_stakeable;
        let nominator_stake = target_staked_value.saturating_sub(initial_validators_balance);
        // Yearly inflation corresponding to 92% staking ratio is 1.4224963%
        let yearly_inflation = Perquintill::from_parts(14_224_963_017_589_600);

        // Send some funds to the nominator
        assert_ok!(Balances::transfer_allow_death(
            RuntimeOrigin::signed(SIGNER),
            NOM_1_STASH,
            nominator_stake,
        ));

        run_to_block(20);

        // Sending bonding transaction
        assert_ok!(Staking::bond(
            RuntimeOrigin::signed(NOM_1_STASH),
            nominator_stake,
            pallet_staking::RewardDestination::Stash
        ));

        run_to_block(30);

        assert_ok!(Staking::nominate(
            RuntimeOrigin::signed(NOM_1_STASH),
            vec![VAL_1_STASH], // nominating "the best" validator
        ));
        let initial_nominators_balance = nominators_total_balance();

        // Running chain until era rollover
        run_to_block(era_duration + 1);

        // No payout is expected for era #0 anyway because the "official" staked amount is 0
        assert_eq!(
            Staking::eras_validator_reward(0)
                .expect("ErasValidatorReward storage must exist after era end; qed"),
            0
        );

        // Test outline:
        // - running chain for almost `<T as pallet_staking::Config>::HistoryDepth` eras;
        // - not claiming any validators rewards in the meantime in order to preserve `stakeable`
        //   and `staked` amounts;
        // - at the end of 84 eras (2 weeks) period claim all rewards and sum up the validators'
        //   and the nominator's balances-in-excess - this would account for all minted funds.
        // - ensure the minted amount corresponds to the 1.4224963% p.a. inflation

        let history_depth = <Test as pallet_staking::Config>::HistoryDepth::get();

        // Running chain for 84 eras
        run_to_block(history_depth as u64 * era_duration + 1);
        // Claim rewards
        for era in 0_u32..history_depth {
            pallet_staking::Validators::<Test>::iter().for_each(|(stash_id, _)| {
                assert_ok!(Staking::payout_stakers(
                    RuntimeOrigin::signed(SIGNER),
                    stash_id,
                    era,
                ));
            });
        }

        // Take up-to-date measurements of the chain stats
        let (total_issuance, total_stakeable, _treasury_balance, rewards_pool_balance) =
            chain_state();

        // Total issuance shouldn't have changed
        assert_eq!(total_issuance, initial_total_issuance);
        // The rewards pool has been used to offset minted rewards
        let actual_rewards = initial_rewards_pool_balance.saturating_sub(rewards_pool_balance);
        let stakeable_delta = total_stakeable.saturating_sub(initial_total_stakeable);
        assert_eq!(actual_rewards, stakeable_delta);

        // Expected
        let overall_time_fraction = Perquintill::from_rational(
            history_depth.saturating_sub(1) as u64 * era_duration_in_millis,
            MILLISECONDS_PER_YEAR,
        );
        let annualized_rewards = yearly_inflation * initial_total_issuance;
        // At 92% staking rate, the respective ROI is within the 30% cap.
        let expected_rewards = overall_time_fraction * annualized_rewards;

        // Rounding error could have accumulated over many eras
        assert_approx_eq!(
            actual_rewards,
            expected_rewards,
            actual_rewards / 10_000 // 0.01%
        );

        let validators_balance_delta =
            validators_total_balance().saturating_sub(initial_validators_balance);
        let nominators_balance_delta =
            nominators_total_balance().saturating_sub(initial_nominators_balance);
        assert_eq!(
            validators_balance_delta + nominators_balance_delta,
            actual_rewards
        );
    });
}

#[test]
fn unclaimed_rewards_burn() {
    init_logger();

    let (target_inflation, ideal_stake, pool_balance, non_stakeable) = sensible_defaults();
    let mut ext = with_parameters(target_inflation, ideal_stake, pool_balance, non_stakeable);
    ext.execute_with(|| {
        // Getting up-to-date data on era duration (they may differ from runtime constants)
        let sessions_per_era = <Test as pallet_staking::Config>::SessionsPerEra::get() as u64;
        let epoch_duration = SESSION_DURATION;
        let era_duration = sessions_per_era * epoch_duration;

        let (initial_total_issuance, initial_total_stakeable, _, initial_rewards_pool_balance) =
            chain_state();
        let initial_validators_balance = validators_total_balance();

        let era_duration_in_millis = era_duration * MILLISECS_PER_BLOCK;

        // Bond and nominate
        run_to_block(10);

        let ideal_staked_value = ideal_stake * initial_total_stakeable;
        let nominator_stake = ideal_staked_value.saturating_sub(initial_validators_balance);

        // Send some funds to the nominator
        assert_ok!(Balances::transfer_allow_death(
            RuntimeOrigin::signed(SIGNER),
            NOM_1_STASH,
            nominator_stake,
        ));

        run_to_block(20);

        // Sending bonding transaction from SIGNER to make 4 npos voters
        assert_ok!(Staking::bond(
            RuntimeOrigin::signed(SIGNER),
            nominator_stake,
            pallet_staking::RewardDestination::Stash
        ));

        run_to_block(30);

        assert_ok!(Staking::nominate(
            RuntimeOrigin::signed(SIGNER),
            vec![VAL_1_STASH], // nominating "the best" validator
        ));

        // Running chain until era rollover
        run_to_block(era_duration + 1);

        // No payout is expected for era #0 anyway because the "official" staked amount is 0
        assert_eq!(
            Staking::eras_validator_reward(0)
                .expect("ErasValidatorReward storage must exist after era end; qed"),
            0
        );

        // Test outline:
        // - running chain for `<T as pallet_staking::Config>::HistoryDepth` eras plus some offset;
        // - not claiming any validators rewards in the meantime in order to preserve `stakeable`
        //   and `staked` amounts;
        // - at the end of the period claim all rewards and sum up the validators' and the
        //   nominator's balances-in-excess;
        // - since we have outdated rewards that account for some percentage of what was due, the
        //   actual rewards received by stakers will add up to only 84% of projected rewards.

        let history_depth = <Test as pallet_staking::Config>::HistoryDepth::get();

        let offset = 16_u32;
        // Running chain for 100 (history_depth + offset) eras
        run_to_block((history_depth.saturating_add(offset) as u64) * era_duration + 1);
        // Claim rewards
        // Attempt to claim stale rewards yields an error
        for era in 0_u32..offset {
            pallet_staking::Validators::<Test>::iter().for_each(|(stash_id, _)| {
                let res = Staking::payout_stakers(RuntimeOrigin::signed(SIGNER), stash_id, era);
                assert!(res.is_err());
                if let Err(e) = res {
                    assert_eq!(
                        e.error,
                        pallet_staking::Error::<Test>::InvalidEraToReward.into()
                    );
                }
            });
        }
        for era in 0_u32..history_depth {
            pallet_staking::Validators::<Test>::iter().for_each(|(stash_id, _)| {
                assert_ok!(Staking::payout_stakers(
                    RuntimeOrigin::signed(SIGNER),
                    stash_id,
                    era + offset,
                ));
            });
        }

        // Take up-to-date measurements of the chain stats
        let (total_issuance, total_stakeable, _treasury_balance, rewards_pool_balance) =
            chain_state();

        // Total issuance shouldn't have changed
        assert_eq!(total_issuance, initial_total_issuance);
        // The rewards pool has been used to offset minted rewards
        let actual_rewards = initial_rewards_pool_balance.saturating_sub(rewards_pool_balance);
        let stakeable_delta = total_stakeable.saturating_sub(initial_total_stakeable);
        assert_eq!(actual_rewards, stakeable_delta);

        // Expected
        let overall_time_fraction = Perquintill::from_rational(
            history_depth.saturating_add(offset) as u64 * era_duration_in_millis,
            MILLISECONDS_PER_YEAR,
        );
        let annualized_rewards = target_inflation * initial_total_issuance;
        let expected_rewards = overall_time_fraction * annualized_rewards;

        // Actual rewards should only amount to 84% (84 eras out of 100) of expected rewards
        assert_approx_eq!(
            actual_rewards,
            Perquintill::from_percent(84) * expected_rewards,
            actual_rewards / 10_000_000 // 0.00001%
        );
    });
}

#[test]
fn empty_rewards_pool_causes_inflation() {
    init_logger();

    let (target_inflation, ideal_stake, _, non_stakeable) = sensible_defaults();
    let pool_balance = 0; // empty rewards pool
    let mut ext = with_parameters(target_inflation, ideal_stake, pool_balance, non_stakeable);
    ext.execute_with(|| {
        // Getting up-to-date data on era duration (they may differ from runtime constants)
        let sessions_per_era = <Test as pallet_staking::Config>::SessionsPerEra::get() as u64;
        let epoch_duration = SESSION_DURATION;
        let era_duration = sessions_per_era * epoch_duration;

        let (initial_total_issuance, _, _, initial_rewards_pool_balance) = chain_state();
        assert_eq!(initial_rewards_pool_balance, 0); // ED is auto-deducted by the getter function

        // Running chain until era rollover
        run_to_block(era_duration + 1);

        // No payout is expected for era #0 anyway because the "official" staked amount is 0
        assert_eq!(
            Staking::eras_validator_reward(0)
                .expect("ErasValidatorReward storage must exist after era end; qed"),
            0
        );

        // Running chain until the next era rollover
        run_to_block(2 * era_duration + 1);

        // Claim rewards to trigger rewards minting
        for era in 0_u32..2 {
            pallet_staking::Validators::<Test>::iter().for_each(|(stash_id, _)| {
                assert_ok!(Staking::payout_stakers(
                    RuntimeOrigin::signed(SIGNER),
                    stash_id,
                    era
                ));
            });
        }

        // Take up-to-date measurements of the chain stats
        let (total_issuance, _, _, rewards_pool_balance) = chain_state();

        // The rewards pool balance is still 0: we should have failed to offset any rewards
        assert_eq!(initial_rewards_pool_balance, rewards_pool_balance);
        // Staker rewards for eras 0 and 1
        let actual_rewards = Staking::eras_validator_reward(1)
            .expect("ErasValidatorReward storage must exist after era end; qed");
        // Total issuance grew accordingly have changed
        assert_eq!(total_issuance, initial_total_issuance + actual_rewards);
    });
}

#[test]
fn election_solution_rewards_add_up() {
    use pallet_election_provider_multi_phase::{Config as MPConfig, RawSolution};
    use sp_npos_elections::ElectionScore;

    init_logger();

    let (target_inflation, ideal_stake, pool_balance, non_stakeable) = sensible_defaults();
    // Solutions submitters
    let accounts = (0_u64..5).map(|i| 100 + i).collect::<Vec<_>>();
    let mut ext = ExtBuilder::<Test>::default()
        .initial_authorities(vec![
            (VAL_1_STASH, VAL_1_AUTH_ID),
            (VAL_2_STASH, VAL_2_AUTH_ID),
            (VAL_3_STASH, VAL_3_AUTH_ID),
        ])
        .stash(VALIDATOR_STAKE)
        .endowment(ENDOWMENT)
        .endowed_accounts(accounts)
        .total_supply(INITIAL_TOTAL_TOKEN_SUPPLY)
        .non_stakeable(non_stakeable)
        .pool_balance(pool_balance)
        .ideal_stake(ideal_stake)
        .target_inflation(target_inflation)
        .build();
    ext.execute_with(|| {
        // Initial chain state
        let (initial_total_issuance, _, initial_treasury_balance, initial_rewards_pool_balance) =
            chain_state();
        assert_eq!(initial_rewards_pool_balance, pool_balance);

        // Running chain until the signing phase begins
        run_to_signed();
        assert!(ElectionProviderMultiPhase::current_phase().is_signed());
        assert_eq!(<Test as MPConfig>::SignedMaxRefunds::get(), 2_u32);
        assert!(<Test as MPConfig>::SignedMaxSubmissions::get() > 3_u32);

        // Submit 3 election solutions candidates:
        // 2 good solutions and 1 incorrect one (with higher score, so that it is going to run
        // through feasibility check as the best candidate but eventually be rejected and slashed).
        let good_solution = RawSolution {
            solution: TestNposSolution {
                votes1: vec![(0, 0), (1, 1), (2, 2)],
                ..Default::default()
            },
            score: ElectionScore {
                minimal_stake: VALIDATOR_STAKE,
                sum_stake: 3 * VALIDATOR_STAKE,
                sum_stake_squared: 3 * VALIDATOR_STAKE * VALIDATOR_STAKE,
            },
            round: 1,
        };
        let bad_solution = RawSolution {
            solution: TestNposSolution {
                votes1: vec![(0, 0), (1, 1), (2, 2)],
                ..Default::default()
            },
            score: ElectionScore {
                minimal_stake: VALIDATOR_STAKE + 100_u128,
                sum_stake: 3 * VALIDATOR_STAKE,
                sum_stake_squared: 3 * VALIDATOR_STAKE * VALIDATOR_STAKE,
            },
            round: 1,
        };
        let solutions = vec![good_solution.clone(), bad_solution, good_solution];
        let solutions_len = solutions.len();
        for (i, s) in solutions.into_iter().enumerate() {
            let account = 100_u64 + i as u64;
            assert_ok!(ElectionProviderMultiPhase::submit(
                RuntimeOrigin::signed(account),
                Box::new(s)
            ));
            assert_eq!(
                Balances::free_balance(account),
                ENDOWMENT - <Test as MPConfig>::SignedDepositBase::convert(solutions_len)
            );
        }

        run_to_unsigned();

        // Measure current stats
        let (total_issuance, _, treasury_balance, rewards_pool_balance) = chain_state();

        // Check all balances consistency:
        // 1. `total_issuance` hasn't change despite rewards having been minted
        assert_eq!(total_issuance, initial_total_issuance);
        // 2. the account whose solution was accepted got reward + tx fee rebate
        assert_eq!(
            Balances::free_balance(102),
            ENDOWMENT
                + <Test as MPConfig>::SignedRewardBase::get()
                + <<Test as MPConfig>::EstimateCallFee as Get<u32>>::get() as u128
        );
        // 3. the account whose solution was rejected got slashed and lost the deposit and fee
        assert_eq!(
            Balances::free_balance(101),
            ENDOWMENT - <Test as MPConfig>::SignedDepositBase::convert(solutions_len)
        );
        // 4. the third account got deposit unreserved and tx fee returned
        assert_eq!(
            Balances::free_balance(100),
            ENDOWMENT + <<Test as MPConfig>::EstimateCallFee as Get<u32>>::get() as u128
        );
        // 5. the slashed deposit went to `Treasury`
        assert_eq!(
            treasury_balance,
            initial_treasury_balance
                + <Test as MPConfig>::SignedDepositBase::convert(solutions_len)
        );
        // 6. the rewards offset pool's balanced decreased to compensate for reward and rebates.
        assert_eq!(
            rewards_pool_balance,
            initial_rewards_pool_balance
                - <Test as MPConfig>::SignedRewardBase::get()
                - <<Test as MPConfig>::EstimateCallFee as Get<u32>>::get() as u128 * 2
        );
    });
}

#[test]
fn rent_pool_disbursments_work() {
    use two_block_producers::*;

    init_logger();

    default_test_ext().execute_with(|| {
        let sessions_per_era = <Test as pallet_staking::Config>::SessionsPerEra::get() as u64;
        let epoch_duration = Session::average_session_length();
        let era_duration = sessions_per_era * epoch_duration;

        // the base reward points value is hardcoded in Substrate but we don't copy it here.
        // A validator has just produced the first block so it has one set of reward points
        // which we can determine.
        let active_era_info = Staking::active_era().unwrap();
        let reward_points = Staking::eras_reward_points(active_era_info.index);
        let (validator, &reward_points) = reward_points.individual.first_key_value().unwrap();
        let reward_points = u64::from(reward_points);

        assert_eq!(era_duration % 2, 0);
        // determine blocks which will be produced by two validators
        // (take into account who has produced the first block)
        let (blocks_1, blocks_2) = {
            let block_count = era_duration / 2;
            match validator {
                &VAL_1_STASH => (block_count + 1, block_count),
                _ => (block_count, block_count + 1),
            }
        };

        // imitate rent charging
        let rent = u128::from(1 + (blocks_1 + blocks_2) * reward_points);
        let imbalance = CurrencyOf::<Test>::issue(rent);
        let result = CurrencyOf::<Test>::resolve_into_existing(
            &StakingRewards::rent_pool_account_id(),
            imbalance,
        );
        assert_ok!(result);

        let free_balance_validator_1 = CurrencyOf::<Test>::free_balance(VAL_1_STASH);
        let free_balance_validator_2 = CurrencyOf::<Test>::free_balance(VAL_2_STASH);
        let free_balance_validator_3 = CurrencyOf::<Test>::free_balance(VAL_3_STASH);

        // go to the next era to trigger payouts
        run_to_block(era_duration + 1);

        // some remaining value cannot be distributed between validators so goes to the next era
        assert_eq!(StakingRewards::rent_pool_balance(), 1);
        // the third validator doesn't produce any blocks
        assert_eq!(
            free_balance_validator_3,
            CurrencyOf::<Test>::free_balance(VAL_3_STASH)
        );
        // the first and the second validators should be rewarded for block producing
        // accordingly to their points
        assert_eq!(
            free_balance_validator_1 + u128::from(blocks_1 * reward_points),
            CurrencyOf::<Test>::free_balance(VAL_1_STASH)
        );
        assert_eq!(
            free_balance_validator_2 + u128::from(blocks_2 * reward_points),
            CurrencyOf::<Test>::free_balance(VAL_2_STASH)
        );
    })
}

fn sensible_defaults() -> (Perquintill, Perquintill, u128, Perquintill) {
    (
        Perquintill::from_rational(578_u64, 10_000_u64),
        Perquintill::from_percent(85),
        Perquintill::from_percent(11) * INITIAL_TOTAL_TOKEN_SUPPLY,
        Perquintill::from_rational(4108_u64, 10_000_u64), // 41.08%
    )
}

fn with_parameters(
    target_inflation: Perquintill,
    ideal_stake: Perquintill,
    pool_balance: u128,
    non_stakeable: Perquintill,
) -> sp_io::TestExternalities {
    ExtBuilder::<Test>::default()
        .initial_authorities(vec![
            (VAL_1_STASH, VAL_1_AUTH_ID),
            (VAL_2_STASH, VAL_2_AUTH_ID),
            (VAL_3_STASH, VAL_3_AUTH_ID),
        ])
        .stash(VALIDATOR_STAKE)
        .endowment(ENDOWMENT)
        .endowed_accounts(vec![SIGNER, NOM_1_STASH])
        .total_supply(INITIAL_TOTAL_TOKEN_SUPPLY)
        .non_stakeable(non_stakeable)
        .pool_balance(pool_balance)
        .ideal_stake(ideal_stake)
        .target_inflation(target_inflation)
        .build()
}
