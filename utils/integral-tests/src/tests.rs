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

use rand::{rngs::SmallRng, seq::SliceRandom, SeedableRng};
use sp_runtime::{MultiAddress, Perbill};

use crate::util::{
    generate_random_authorities, init_logger, nominator_keys_from_seed, nominators_total_balance,
    run_for_n_blocks, run_to_block_with_offchain, set_balance_proposal, validators_total_balance,
    vote_aye, vote_nay, Arc, Balances, Democracy, ExtBuilder, Lazy, NominatorAccountId, PoolState,
    Runtime, RuntimeOrigin, RwLock, Staking, Treasury, ValidatorAccountId, EXISTENTIAL_DEPOSIT,
    MILLISECS_PER_BLOCK, SIGNING_KEY, TOKEN,
};
use frame_support::assert_ok;
use pallet_collective::{Instance1, Instance2, Origin};
use sp_std::collections::btree_map::BTreeMap;

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

const MILLISECONDS_PER_YEAR: u64 = 1000 * 3600 * 24 * 36525 / 100; // as of Substrate staking pallet
const ENDOWMENT: u128 = 100 * TOKEN;
static MIN_INFLATION: Perbill = Perbill::from_perthousand(25_u32); // 2.5%
static MAX_INFLATION: Perbill = Perbill::from_perthousand(75_u32); // 7.5%
static COMPARISON_TOLERANCE_RATIO: Perbill = Perbill::from_perthousand(1_u32); // 0.1%

#[derive(Debug, Clone, Default)]
pub struct TestParams {
    epoch_duration: u64,
    validators: Vec<ValidatorAccountId>,
    nominators: Vec<NominatorAccountId>,
    seed: u32,
}

impl TestParams {
    fn with_validators(validators: Vec<ValidatorAccountId>) -> Self {
        Self {
            epoch_duration: 250, // 250 blocks per session => 1500 block per era
            validators: validators,
            nominators: Default::default(),
            seed: 1000,
        }
    }
}

pub fn run_test(params: &TestParams, f: impl FnOnce(&TestParams, &Arc<RwLock<PoolState>>)) {
    init_logger();

    log::debug!("Running test with params = {:?}", params);
    let builder = ExtBuilder::default();
    let (mut ext, pool) = builder
        .seed(params.seed)
        .initial_authorities(params.validators.clone())
        .stash(10 * EXISTENTIAL_DEPOSIT)
        .endowment(ENDOWMENT)
        .endowed_accounts(
            params
                .nominators
                .iter()
                .map(|x| x.0.clone())
                .chain(vec![Treasury::account_id(), (&*SIGNING_KEY).clone()])
                .collect(),
        )
        .epoch_duration(params.epoch_duration)
        .build_with_offchain();
    ext.execute_with(|| f(params, &pool));
}

#[test]
fn inflation_without_nominators() {
    let validators_sets = vec![5, 10, 15];
    for vs in validators_sets {
        let params = TestParams::with_validators(generate_random_authorities(vs));
        run_test(&params, |p, pool| {
            // Getting up to date data on era duration (they may differ from runtime constants)
            let sessions_per_era = <Runtime as pallet_staking::Config>::SessionsPerEra::get();
            let epoch_duration = <Runtime as pallet_babe::Config>::EpochDuration::get() as u32;
            let era_duration = sessions_per_era * epoch_duration;

            let num_validators = p.validators.len() as u32;
            let initial_total_issuance = Balances::total_issuance();
            let initial_treasury_balance = Balances::free_balance(Treasury::account_id());
            let signer = Lazy::get(&SIGNING_KEY).expect("value initialized; qed");
            let signer_balance = Balances::free_balance(signer);
            assert_eq!(
                initial_total_issuance,
                // Each validator has (10 * EXISTENTIAL_DEPOSIT)
                (num_validators * 10) as u128 * EXISTENTIAL_DEPOSIT
                    + initial_treasury_balance
                    + signer_balance
            );

            // Validators payout:
            //         `yearly_inlfation(staked_amount)` * `total_issuance` * `era_duration_in_millis`
            //    P = ---------------------------------------------------------------------------------
            //                                 `milliseconds_in_year`
            //
            // Inflation:
            //         `yearly_inlfation(ideal_stake)` * `total_issuance` * `era_duration_in_millis`
            //    I = -------------------------------------------------------------------------------
            //                                 `milliseconds_in_year`
            //
            // Remainder (goes to treasury):
            //
            //    R = I - P

            let inflation_curve_slope: Perbill = Perbill::from_rational(1_u32, 15_u32); // (`max_inflation` - `min_inflation`) / `ideal_stake`
            let inflation_curve_intercept: Perbill = MIN_INFLATION;

            let era_duration_in_millis = era_duration as u64 * MILLISECS_PER_BLOCK;
            let fraction = Perbill::from_rational(era_duration_in_millis, MILLISECONDS_PER_YEAR);
            let yearly_inflation_at_0 = MIN_INFLATION;

            // Running chain until era rollover
            run_to_block_with_offchain(era_duration + 1, pool);

            let expected_payout_0 = yearly_inflation_at_0 * (fraction * initial_total_issuance);
            let max_inflation = MAX_INFLATION * (fraction * initial_total_issuance);
            let expected_remainder = max_inflation.saturating_sub(expected_payout_0);

            // At this point `expected_remainder` has been minted to Treasury
            let total_issuance = Balances::total_issuance();
            let treasury_balance = Balances::free_balance(Treasury::account_id());
            assert_eq!(total_issuance, initial_total_issuance + expected_remainder);
            assert_eq!(
                treasury_balance,
                initial_treasury_balance + expected_remainder
            );
            // Validators reward stored in pallet_staking storage
            let validators_reward_for_era = Staking::eras_validator_reward(0)
                .expect("ErasValidatorReward storage must exist after era end; qed");
            assert_eq!(validators_reward_for_era, expected_payout_0);

            // Running chain until the next era rollover
            let initial_total_issuance = total_issuance;
            let initial_treasury_balance = treasury_balance;
            run_to_block_with_offchain(2 * era_duration + 1, pool);

            let total_staked = num_validators as u128 * 10 * EXISTENTIAL_DEPOSIT;
            assert_eq!(total_staked, Staking::eras_total_stake(1));

            let yearly_inflation_at_1 = inflation_curve_slope
                * Perbill::from_rational(total_staked, total_issuance)
                + inflation_curve_intercept;

            let expected_payout_1 = yearly_inflation_at_1 * (fraction * initial_total_issuance);
            let max_inflation = MAX_INFLATION * (fraction * initial_total_issuance);
            let expected_remainder = max_inflation.saturating_sub(expected_payout_1);

            // `expected_remainder` has, again, already been minted to Treasury
            let total_issuance = Balances::total_issuance();
            let treasury_balance = Balances::free_balance(Treasury::account_id());
            assert_approx_eq!(
                total_issuance,
                initial_total_issuance + expected_remainder,
                COMPARISON_TOLERANCE_RATIO * COMPARISON_TOLERANCE_RATIO * total_issuance
            );
            assert_approx_eq!(
                treasury_balance,
                initial_treasury_balance + expected_remainder,
                COMPARISON_TOLERANCE_RATIO * COMPARISON_TOLERANCE_RATIO * treasury_balance
            );
            // Validators reward stored in pallet_staking storage
            let validators_reward_for_era = Staking::eras_validator_reward(1)
                .expect("ErasValidatorReward storage must exist after era end; qed");
            assert_approx_eq!(
                validators_reward_for_era,
                expected_payout_1,
                COMPARISON_TOLERANCE_RATIO * COMPARISON_TOLERANCE_RATIO * validators_reward_for_era
            );

            // Trigger validators payout
            for era in 0_u32..2 {
                pallet_staking::Validators::<Runtime>::iter().for_each(|(stash_id, _)| {
                    assert_ok!(Staking::payout_stakers(
                        RuntimeOrigin::signed(signer.clone()),
                        stash_id,
                        era,
                    ));
                });
            }

            // Validators rewards all ended up at validators' stashes thereby increasing total issuance
            let initial_total_issuance = total_issuance;
            let total_issuance = Balances::total_issuance();
            assert_approx_eq!(
                total_issuance,
                initial_total_issuance + expected_payout_0 + expected_payout_1,
                COMPARISON_TOLERANCE_RATIO * COMPARISON_TOLERANCE_RATIO * total_issuance
            );
        });
    }
}

#[test]
fn inflation_with_nominators() {
    let validators_set = vec![5, 10, 15];
    for vs in validators_set {
        let nominators_set = vec![20, 30, 50];
        for ns in nominators_set {
            let mut nominators = vec![];
            for i in 0..ns {
                nominators.push(nominator_keys_from_seed(&format!("nominator{}", i)));
            }
            let params = TestParams {
                epoch_duration: 250,
                validators: generate_random_authorities(vs),
                nominators: nominators,
                seed: 1000,
            };

            run_test(&params, |p, pool| {
                // Getting up to date data on era duration (they may differ from runtime constants)
                let sessions_per_era = <Runtime as pallet_staking::Config>::SessionsPerEra::get();
                let epoch_duration = <Runtime as pallet_babe::Config>::EpochDuration::get() as u32;
                let era_duration = sessions_per_era * epoch_duration;

                let num_validators = p.validators.len() as u32;
                let num_nominators = p.nominators.len() as u32;
                let mut rng = SmallRng::seed_from_u64(p.seed as u64);

                let initial_total_issuance = Balances::total_issuance();
                let initial_treasury_balance = Balances::free_balance(Treasury::account_id());
                let signer = Lazy::get(&SIGNING_KEY).expect("value initialized; qed");
                let signer_balance = Balances::free_balance(signer);
                let initial_nominators_balance = nominators_total_balance(p.nominators.clone());
                assert_eq!(
                    initial_total_issuance,
                    // num_validators + root have (10 * EXISTENTIAL_DEPOSIT) each
                    (num_validators * 10) as u128 * EXISTENTIAL_DEPOSIT
                        + p.nominators.len() as u128 * ENDOWMENT
                        + initial_treasury_balance
                        + signer_balance
                );

                let inflation_curve_slope: Perbill = Perbill::from_rational(1_u32, 15_u32); // (`max_inflation` - `min_inflation`) / `ideal_stake`
                let inflation_curve_intercept: Perbill = MIN_INFLATION;

                let era_duration_in_millis = era_duration as u64 * MILLISECS_PER_BLOCK;
                let fraction =
                    Perbill::from_rational(era_duration_in_millis, MILLISECONDS_PER_YEAR);
                let yearly_inflation_at_0 = MIN_INFLATION;

                // Bond and nominate
                run_to_block_with_offchain(100, pool);
                for nominator in &p.nominators {
                    let (stash_id, controller_id) = nominator;
                    // Sending bonding transaction
                    assert_ok!(Staking::bond(
                        RuntimeOrigin::signed(stash_id.clone()),
                        MultiAddress::Id(controller_id.clone()),
                        ENDOWMENT.saturating_div(2),
                        pallet_staking::RewardDestination::Stash
                    ));
                }
                run_to_block_with_offchain(200, pool);
                for nominator in &p.nominators {
                    let (_, controller_id) = nominator;
                    let validator_id = &p
                        .validators
                        .choose(&mut rng)
                        .expect("Validators vec must not be empty; qued")
                        .0; // validator's stash
                            // Nominating transaction
                    assert_ok!(Staking::nominate(
                        RuntimeOrigin::signed(controller_id.clone()),
                        vec![MultiAddress::Id(validator_id.clone())],
                    ));
                }

                // Running chain until era rollover
                run_to_block_with_offchain(era_duration + 1, pool);

                let expected_payout_0 = yearly_inflation_at_0 * (fraction * initial_total_issuance);
                let max_inflation = MAX_INFLATION * (fraction * initial_total_issuance);
                let expected_remainder = max_inflation.saturating_sub(expected_payout_0);

                // At this point `expected_remainder` has been minted to Treasury
                let total_issuance = Balances::total_issuance();
                let treasury_balance = Balances::free_balance(Treasury::account_id());
                assert_eq!(total_issuance, initial_total_issuance + expected_remainder);
                assert_eq!(
                    treasury_balance,
                    initial_treasury_balance + expected_remainder
                );
                // Validators reward stored in pallet_staking storage
                let validators_reward_for_era = Staking::eras_validator_reward(0)
                    .expect("ErasValidatorReward storage must exist after era end; qed");
                assert_eq!(validators_reward_for_era, expected_payout_0);
                // Nominators total balance of stash accounts is still the same as before
                assert_eq!(
                    nominators_total_balance(p.nominators.clone()),
                    num_nominators as u128 * ENDOWMENT
                );

                // Running chain until the next era rollover
                let initial_total_issuance = total_issuance;
                let initial_treasury_balance = treasury_balance;
                run_to_block_with_offchain(2 * era_duration + 1, pool);

                // Staked amount now consists of validators and nominators' stake
                let validators_own_stake = num_validators as u128 * 10 * EXISTENTIAL_DEPOSIT;
                let validators_other_stake = num_nominators as u128 * ENDOWMENT.saturating_div(2);
                let total_staked = validators_own_stake + validators_other_stake;
                assert_eq!(total_staked, Staking::eras_total_stake(1));

                let yearly_inflation_at_1 = inflation_curve_slope
                    * Perbill::from_rational(total_staked, total_issuance)
                    + inflation_curve_intercept;

                let expected_payout_1 = yearly_inflation_at_1 * (fraction * initial_total_issuance);
                let max_inflation = MAX_INFLATION * (fraction * initial_total_issuance);
                let expected_remainder = max_inflation.saturating_sub(expected_payout_1);

                // `expected_remainder` has, again, already been minted to Treasury
                let total_issuance = Balances::total_issuance();
                let treasury_balance = Balances::free_balance(Treasury::account_id());
                assert_approx_eq!(
                    total_issuance,
                    initial_total_issuance + expected_remainder,
                    COMPARISON_TOLERANCE_RATIO * COMPARISON_TOLERANCE_RATIO * total_issuance
                );
                assert_approx_eq!(
                    treasury_balance,
                    initial_treasury_balance + expected_remainder,
                    COMPARISON_TOLERANCE_RATIO * COMPARISON_TOLERANCE_RATIO * treasury_balance
                );
                // Validators reward stored in pallet_staking storage
                let validators_reward_for_era = Staking::eras_validator_reward(1)
                    .expect("ErasValidatorReward storage must exist after era end; qed");
                assert_approx_eq!(
                    validators_reward_for_era,
                    expected_payout_1,
                    COMPARISON_TOLERANCE_RATIO
                        * COMPARISON_TOLERANCE_RATIO
                        * validators_reward_for_era
                );

                // Trigger validators payout
                for era in 0_u32..2 {
                    pallet_staking::Validators::<Runtime>::iter().for_each(|(stash_id, _)| {
                        assert_ok!(Staking::payout_stakers(
                            RuntimeOrigin::signed(signer.clone()),
                            stash_id,
                            era,
                        ));
                    });
                }

                // Validators rewards all ended up at validators' stashes thereby increasing total issuance
                let initial_total_issuance = total_issuance;
                let total_issuance = Balances::total_issuance();
                // Allow for some rounding error
                assert_approx_eq!(
                    total_issuance,
                    initial_total_issuance + expected_payout_0 + expected_payout_1,
                    COMPARISON_TOLERANCE_RATIO * total_issuance
                );

                let nominators_balance_diff = nominators_total_balance(p.nominators.clone())
                    .saturating_sub(initial_nominators_balance);

                // We only can estimate nominators' cut on average: longer run gives more accurate estimation
                // A rough estimation of nominators' share is other_staked / total_staked
                let fraction = Perbill::from_rational(validators_other_stake, total_staked);
                let expected_nominators_payout = fraction * expected_payout_1;
                assert_approx_eq!(
                    nominators_balance_diff,
                    expected_nominators_payout,
                    COMPARISON_TOLERANCE_RATIO * expected_nominators_payout * 150 // allowing 15% tolerance due to intrinsic imprecision
                );
            });
        }
    }
}

#[test]
fn inflation_over_long_run() {
    let mut nominators = vec![];
    for i in 0..8 {
        nominators.push(nominator_keys_from_seed(&format!("nominator{}", i)));
    }
    let params = TestParams {
        epoch_duration: 250,
        validators: generate_random_authorities(5),
        nominators: nominators,
        seed: 1000,
    };

    run_test(&params, |p, pool| {
        // Getting up to date data on era duration (they may differ from runtime constants)
        let sessions_per_era = <Runtime as pallet_staking::Config>::SessionsPerEra::get();
        let epoch_duration = <Runtime as pallet_babe::Config>::EpochDuration::get() as u32;
        let era_duration = sessions_per_era * epoch_duration;

        let mut rng = SmallRng::seed_from_u64(p.seed as u64);

        let initial_total_issuance = Balances::total_issuance();
        let signer = Lazy::get(&SIGNING_KEY).expect("value initialized; qed");

        let era_duration_in_millis = era_duration as u64 * MILLISECS_PER_BLOCK;
        let fraction = Perbill::from_rational(era_duration_in_millis, MILLISECONDS_PER_YEAR);

        // Bond and nominate
        run_to_block_with_offchain(100, pool);
        for nominator in &p.nominators {
            let (stash_id, controller_id) = nominator;
            // Sending bonding transaction
            assert_ok!(Staking::bond(
                RuntimeOrigin::signed(stash_id.clone()),
                MultiAddress::Id(controller_id.clone()),
                ENDOWMENT.saturating_div(2),
                pallet_staking::RewardDestination::Stash
            ));
        }
        run_to_block_with_offchain(200, pool);
        for nominator in &p.nominators {
            let (_, controller_id) = nominator;
            let validator_id = &p
                .validators
                .choose(&mut rng)
                .expect("Validators vec must not be empty; qued")
                .0; // validator's stash
                    // Nominating transaction
            assert_ok!(Staking::nominate(
                RuntimeOrigin::signed(controller_id.clone()),
                vec![MultiAddress::Id(validator_id.clone())],
            ));
        }

        let mut total_issuance = initial_total_issuance;
        let mut total_inflation = 0_u128;

        // Running chain for 100 eras
        for era_index in 0..100_u32 {
            run_to_block_with_offchain(era_duration * (era_index + 1) + 1, pool);
            // Treasury's part should have been minted to contribute to the `total_issuence`
            // Catch up with validators' payout for the era
            pallet_staking::Validators::<Runtime>::iter().for_each(|(stash_id, _)| {
                assert_ok!(Staking::payout_stakers(
                    RuntimeOrigin::signed(signer.clone()),
                    stash_id,
                    era_index,
                ));
            });

            total_inflation += MAX_INFLATION * (fraction * total_issuance);
            total_issuance = Balances::total_issuance();
        }

        // Allow for cumulative rounding error
        assert_approx_eq!(
            total_issuance,
            initial_total_issuance + total_inflation,
            COMPARISON_TOLERANCE_RATIO * total_issuance
        );
    });
}

#[test]
fn unclaimed_rewards_burn() {
    let mut nominators = vec![];
    for i in 0..10 {
        nominators.push(nominator_keys_from_seed(&format!("nominator{}", i)));
    }
    let params = TestParams {
        epoch_duration: 250,
        validators: generate_random_authorities(5),
        nominators: nominators,
        seed: 1000,
    };

    run_test(&params, |p, pool| {
        // Getting up to date data on era duration (they may differ from runtime constants)
        let sessions_per_era = <Runtime as pallet_staking::Config>::SessionsPerEra::get();
        let epoch_duration = <Runtime as pallet_babe::Config>::EpochDuration::get() as u32;
        let era_duration = sessions_per_era * epoch_duration;

        let mut rng = SmallRng::seed_from_u64(p.seed as u64);
        let num_validators = p.validators.len() as u32;
        let num_nominators = p.nominators.len() as u32;

        let signer = Lazy::get(&SIGNING_KEY).expect("value initialized; qed");

        let inflation_curve_slope: Perbill = Perbill::from_rational(1_u32, 15_u32); // (`max_inflation` - `min_inflation`) / `ideal_stake`
        let inflation_curve_intercept: Perbill = MIN_INFLATION;

        let era_duration_in_millis = era_duration as u64 * MILLISECS_PER_BLOCK;
        let fraction = Perbill::from_rational(era_duration_in_millis, MILLISECONDS_PER_YEAR);
        let yearly_inflation_at_0 = MIN_INFLATION;
        // From era 1 onwards totale staked amount will matter
        let validators_own_stake = num_validators as u128 * 10 * EXISTENTIAL_DEPOSIT;
        let validators_other_stake = num_nominators as u128 * ENDOWMENT.saturating_div(2);

        // Bond and nominate
        for nominator in &p.nominators {
            let (stash_id, controller_id) = nominator;
            // Sending bonding transaction
            assert_ok!(Staking::bond(
                RuntimeOrigin::signed(stash_id.clone()),
                MultiAddress::Id(controller_id.clone()),
                ENDOWMENT.saturating_div(2),
                pallet_staking::RewardDestination::Stash
            ));
        }
        run_to_block_with_offchain(1, pool);
        for nominator in &p.nominators {
            let (_, controller_id) = nominator;
            let validator_id = &p
                .validators
                .choose(&mut rng)
                .expect("Validators vec must not be empty; qued")
                .0; // validator's stash
                    // Nominating transaction
            assert_ok!(Staking::nominate(
                RuntimeOrigin::signed(controller_id.clone()),
                vec![MultiAddress::Id(validator_id.clone())],
            ));
        }

        let mut expected_payouts = BTreeMap::new();
        let history_depth = <Runtime as pallet_staking::Config>::HistoryDepth::get();
        let mut total_issuance = Balances::total_issuance();
        let mut total_staked = validators_own_stake + validators_other_stake;

        // Running chain for a few eras first (less than history depth)
        for era_index in 0_u32..history_depth.saturating_sub(4) {
            run_for_n_blocks(era_duration);

            let yearly_inflation = match era_index {
                0_u32 => yearly_inflation_at_0,
                _ => {
                    inflation_curve_slope * Perbill::from_rational(total_staked, total_issuance)
                        + inflation_curve_intercept
                }
            };
            let expected_reward = yearly_inflation * (fraction * total_issuance);
            if let Some(actual_reward) = Staking::eras_validator_reward(era_index) {
                // Since we use different arithmetics here comparing to `staking` pallet
                // we can expect some small rounding error to crop in
                assert_approx_eq!(
                    expected_reward,
                    actual_reward,
                    COMPARISON_TOLERANCE_RATIO * COMPARISON_TOLERANCE_RATIO * actual_reward // 10^-6 of the absolute value
                );
            }
            expected_payouts.insert(era_index, expected_reward);
            total_issuance = Balances::total_issuance();
        }

        // All validators and nominators consolidated balances before payouts
        let validators_balance = validators_total_balance(p.validators.clone());
        let nominators_balance = nominators_total_balance(p.nominators.clone());

        // Trigger validators payout; all eras so far should be rewarded
        for era in 0_u32..history_depth.saturating_sub(4) {
            pallet_staking::Validators::<Runtime>::iter().for_each(|(stash_id, _)| {
                assert_ok!(Staking::payout_stakers(
                    RuntimeOrigin::signed(signer.clone()),
                    stash_id,
                    era,
                ));
            });
        }

        let nett_payout = validators_total_balance(p.validators.clone())
            .saturating_sub(validators_balance)
            + nominators_total_balance(p.nominators.clone()).saturating_sub(nominators_balance);
        let expected_total_payout = expected_payouts
            .iter()
            .map(|(_, v)| *v)
            .fold(0_u128, |acc, x| acc.saturating_add(x));

        // Allow for cumulative rounding error
        // TODO: make sure this is, indeed, rounding error and not something else
        assert_approx_eq!(
            nett_payout,
            expected_total_payout,
            COMPARISON_TOLERANCE_RATIO * COMPARISON_TOLERANCE_RATIO * nett_payout
        );

        let mut current_era = Staking::current_era().expect("sessions have been rotating; qed");
        assert_eq!(current_era, history_depth.saturating_sub(4));
        let start_era = current_era;
        total_issuance = Balances::total_issuance();

        // Now run the chain for a period exceeding history depth
        let eras_to_run = history_depth + 10;
        for _era_index in 0_u32..eras_to_run {
            run_for_n_blocks(era_duration);

            let yearly_inflation = inflation_curve_slope
                * Perbill::from_rational(total_staked, total_issuance)
                + inflation_curve_intercept;
            let expected_reward = yearly_inflation * (fraction * total_issuance);
            if let Some(actual_reward) = Staking::eras_validator_reward(current_era) {
                // Since we use different arithmetics here comparing to `staking` pallet
                // we can expect some small rounding error to crop in
                assert_approx_eq!(
                    expected_reward,
                    actual_reward,
                    COMPARISON_TOLERANCE_RATIO * COMPARISON_TOLERANCE_RATIO * actual_reward // 10^-6 of the absolute value
                );
            }
            expected_payouts.insert(current_era, expected_reward);
            current_era += 1;
            total_issuance = Balances::total_issuance();
            total_staked = Staking::eras_total_stake(current_era);
        }

        // All validators and nominators consolidated balances before payouts
        let validators_balance = validators_total_balance(p.validators.clone());
        let nominators_balance = nominators_total_balance(p.nominators.clone());

        // Trigger validators payout
        for era in 0_u32..eras_to_run {
            let era_index = start_era + era;
            pallet_staking::Validators::<Runtime>::iter().for_each(|(stash_id, _)| {
                if era_index < (current_era - history_depth) {
                    // for eras that are older than history depth, expecting error
                    // N.B. `assert_noop!` or `asser_err!` do not work here because the extrinsic
                    // returns `Some(acutal_weight)` in PostDispatchInfo
                    assert!(Staking::payout_stakers(
                        RuntimeOrigin::signed(signer.clone()),
                        stash_id,
                        era_index,
                    )
                    .is_err());
                } else {
                    assert_ok!(Staking::payout_stakers(
                        RuntimeOrigin::signed(signer.clone()),
                        stash_id,
                        era_index,
                    ));
                }
            });
        }

        let nett_payout = validators_total_balance(p.validators.clone())
            .saturating_sub(validators_balance)
            + nominators_total_balance(p.nominators.clone()).saturating_sub(nominators_balance);
        // Payout that would have been accumulated from era 20 onwards with bigger history depth
        let expected_payout_without_cutoff = expected_payouts
            .iter()
            .filter(|(k, _)| **k >= start_era)
            .map(|(_, v)| v)
            .fold(0_u128, |acc, x| acc.saturating_add(*x));

        // However, actual nett payout is less than that by some significant amount
        assert!(nett_payout < expected_payout_without_cutoff - 1_000_000);

        // Expected payout collected within the history depth
        let expected_total_payout = expected_payouts
            .into_iter()
            .filter(|(k, _)| *k + history_depth >= current_era)
            .map(|(_, v)| v)
            .fold(0_u128, |acc, x| acc.saturating_add(x));

        // Allow for cumulative rounding error
        // TODO: make sure this is, indeed, rounding error and not something else
        assert_approx_eq!(
            nett_payout,
            expected_total_payout,
            COMPARISON_TOLERANCE_RATIO * COMPARISON_TOLERANCE_RATIO * nett_payout
        );
    });
}

#[test]
fn fasttrack_proposal_works() {
    let mut nominators = vec![];
    for i in 0..14 {
        nominators.push(nominator_keys_from_seed(&format!("nominator{}", i)));
    }
    let params = TestParams {
        epoch_duration: 250,
        validators: generate_random_authorities(10),
        nominators: nominators,
        seed: 1000,
    };

    run_test(&params, |p, pool| {
        let num_endowed_accounts = p.nominators.len() + 2;

        let technical_committee = p.nominators[0..(num_endowed_accounts + 1) / 2].to_vec();
        let num_members = technical_committee.len() as u32;

        let signer = Lazy::get(&SIGNING_KEY).expect("value initialized; qed");

        let h = set_balance_proposal(signer.clone(), 10 * TOKEN).hash();

        let voting_period = <Runtime as pallet_democracy::Config>::VotingPeriod::get() + 10; // Should exceed `VotingPeriod` to avoid instant_origin check

        let tech_comm_origin = Origin::<Runtime, Instance2>::Members(6_u32, num_members);
        let council_origin = Origin::<Runtime, Instance1>::Members(6_u32, num_members);

        // Creating a proposal first
        assert_ok!(Democracy::external_propose_majority(
            council_origin.clone().into(),
            set_balance_proposal(signer.clone(), 10 * TOKEN),
        ));

        assert_ok!(Democracy::fast_track(
            tech_comm_origin.clone().into(),
            h,
            voting_period,
            0
        ));

        let ref_index = 0;
        assert_eq!(Democracy::lowest_unbaked(), ref_index);

        assert_ok!(Democracy::vote(
            RuntimeOrigin::signed(signer.clone()),
            ref_index,
            vote_nay(2 * TOKEN),
        ));
        for n in &p.nominators {
            assert_ok!(Democracy::vote(
                RuntimeOrigin::signed(n.0.clone()),
                ref_index,
                vote_aye(2 * TOKEN),
            ));
        }

        // Running chain until just before the referendum should have matured
        run_to_block_with_offchain(voting_period, pool);
        assert_eq!(Democracy::lowest_unbaked(), ref_index);
        // Now referendum is about to be baked
        run_for_n_blocks(1);
        assert_eq!(Democracy::lowest_unbaked(), ref_index + 1);
        // Scheduled call dispatch should have taken place
        run_for_n_blocks(1);
        assert_eq!(Balances::free_balance(signer.clone()), 10 * TOKEN);
    });
}
