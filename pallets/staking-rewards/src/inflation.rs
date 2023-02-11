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

//! This module exposes a function which returns the total payout for the era given
//! the era duration and the staking rate.
//! The staking rate in NPoS is the total amount of tokens staked by nominators and validators,
//! divided by the total STAKEABLE tokens amount (as opposed to the total token supply).

use pallet_staking_reward_fn::compute_inflation;
use sp_runtime::{
    traits::{AtLeast32BitUnsigned, CheckedDiv},
    FixedU128, PerThing, Perquintill, Saturating,
};

/// The total payout to all validators (and their nominators) per era and
/// payout in excess that is sent to Treasury.
#[allow(clippy::too_many_arguments)]
pub fn compute_total_payout<Balance>(
    total_staked: Balance,
    total_stakeable: Balance,
    total_token_supply: Balance,
    ideal_stake: Perquintill,
    min_annual_inflation: Perquintill,
    max_annual_inflation: Perquintill,
    falloff: Perquintill,
    max_roi: Perquintill,
    period_fraction: Perquintill,
) -> (Balance, Balance)
where
    Balance: AtLeast32BitUnsigned + Clone,
{
    let delta_annual_inflation = max_annual_inflation.saturating_sub(min_annual_inflation);

    let total_staked: u128 = total_staked.unique_saturated_into();
    let total_stakeable: u128 = total_stakeable.unique_saturated_into();
    let total_token_supply: u128 = total_token_supply.unique_saturated_into();

    let stake = Perquintill::from_rational(total_staked, total_stakeable);

    // Compute inflation normalized to 1
    let normalized_inflation = compute_inflation(stake, ideal_stake, falloff);
    let actual_inflation =
        min_annual_inflation.saturating_add(delta_annual_inflation * normalized_inflation);

    let annualized_payout = actual_inflation * total_token_supply;
    let total_payout = period_fraction * annualized_payout;

    if total_staked == 0 {
        // Practically impossible case as there is always a lower bound for validators' stake.
        // However, for the sake of uniformity, in this case we may argue that since the
        // expected stakers' ROI is going to be infinite, the entire payout should go to Treasury
        // (or whatever "magic" account is assigned for this purpose).
        // Validators rewards can later be drawn from this "magic" account.
        return (
            Balance::zero(),
            Balance::unique_saturated_from(total_payout),
        );
    }

    // `annualized_roi` = `annualized_payout` / `total_staked`
    let annualized_roi = FixedU128::from_rational(annualized_payout, total_staked);
    let roi_cap =
        FixedU128::from_rational(max_roi.deconstruct().into(), Perquintill::ACCURACY.into());

    if annualized_roi < roi_cap {
        // the whole lot goes to validators
        return (
            Balance::unique_saturated_from(total_payout),
            Balance::zero(),
        );
    }

    let reward_fraction = roi_cap
        .checked_div(&annualized_roi)
        .unwrap_or(FixedU128::from(0));

    // converting rewards fractions back to Perquintill
    let reward_fraction: Perquintill = reward_fraction.into_clamped_perthing();

    let staking_payout = reward_fraction * total_payout;
    let rest = total_payout.saturating_sub(staking_payout);

    (
        Balance::unique_saturated_from(staking_payout),
        Balance::unique_saturated_from(rest),
    )
}

#[cfg(test)]
mod test {
    use sp_runtime::Perquintill;

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

    static TTS: u128 = 10_000_000_000;
    static STAKEABLE: u128 = 4_250_000_000; // 42.5% of TTS
    static IDEAL_STAKE: Perquintill = Perquintill::from_percent(85);
    static MIN_INFLATION: Perquintill = Perquintill::from_percent(1);
    static MAX_INFLATION: Perquintill = Perquintill::from_parts(57_800_000_000_000_000_u64); // 5.78%
    static FALLOFF: Perquintill = Perquintill::from_percent(2);
    static MAX_ROI: Perquintill = Perquintill::from_percent(30);
    static MILLISECONDS_PER_YEAR: u64 = 1000 * 3600 * 24 * 36525 / 100;

    #[test]
    fn annual_inflation_is_correct() {
        let time_fraction = Perquintill::from_percent(100); // whole year

        let staked = (0_u64..=20)
            .map(|p| Perquintill::from_percent(p * 5))
            .collect::<Vec<_>>();

        let expected_payouts = vec![
            (0, 100_000_000),
            (63_750_000, 64_367_647),
            (127_500_000, 28_735_294),
            (184_352_941, 0),
            (212_470_588, 0),
            (240_588_235, 0),
            (268_705_882, 0),
            (296_823_529, 0),
            (324_941_176, 0),
            (353_058_824, 0),
            (381_176_471, 0),
            (409_294_118, 0),
            (437_411_765, 0),
            (465_529_412, 0),
            (493_647_059, 0),
            (521_764_706, 0),
            (549_882_353, 0),
            (578_000_000, 0),
            (184_499_260, 0),
            (114_937_500, 0),
            (102_640_602, 0),
        ];
        assert_eq!(staked.len(), expected_payouts.len());

        for (p, expected) in staked.into_iter().zip(expected_payouts.into_iter()) {
            assert_eq!(
                super::compute_total_payout(
                    p * STAKEABLE,
                    STAKEABLE,
                    TTS,
                    IDEAL_STAKE,
                    MIN_INFLATION,
                    MAX_INFLATION,
                    FALLOFF,
                    MAX_ROI,
                    time_fraction,
                ),
                expected
            );
        }
    }

    #[test]
    fn daily_inflation_is_correct() {
        const DAY: u64 = 1000 * 3600 * 24;
        let time_fraction = Perquintill::from_rational(DAY, MILLISECONDS_PER_YEAR);

        let staked = (0_u64..=20)
            .map(|p| Perquintill::from_percent(p * 5))
            .collect::<Vec<_>>();

        let expected_payouts = vec![
            (0, 273_785),
            (174_538, 176_229),
            (349_076, 78_673),
            (504_731, 0),
            (581_713, 0),
            (658_695, 0),
            (735_677, 0),
            (812_659, 0),
            (889_640, 0),
            (966_622, 0),
            (1_043_604, 0),
            (1_120_586, 0),
            (1_197_568, 0),
            (1_274_550, 0),
            (1_351_532, 0),
            (1_428_514, 0),
            (1_505_496, 0),
            (1_582_478, 0),
            (505_131, 0),
            (314_682, 0),
            (281_015, 0),
        ];
        assert_eq!(staked.len(), expected_payouts.len());

        for (p, expected) in staked.into_iter().zip(expected_payouts.into_iter()) {
            assert_eq!(
                super::compute_total_payout(
                    p * STAKEABLE,
                    STAKEABLE,
                    TTS,
                    IDEAL_STAKE,
                    MIN_INFLATION,
                    MAX_INFLATION,
                    FALLOFF,
                    MAX_ROI,
                    time_fraction,
                ),
                expected
            );
        }
    }

    #[test]
    fn four_hourly_inflation_is_correct() {
        const FOUR_HOURS: u64 = 1000 * 3600 * 4;
        let time_fraction = Perquintill::from_rational(FOUR_HOURS, MILLISECONDS_PER_YEAR);

        let staked = (0_u64..=20)
            .map(|p| Perquintill::from_percent(p * 5))
            .collect::<Vec<_>>();

        let expected_payouts = vec![
            (0, 45_631),
            (29_090, 29_372),
            (58_179, 13_112),
            (84_122, 0),
            (96_952, 0),
            (109_782, 0),
            (122_613, 0),
            (135_443, 0),
            (148_273, 0),
            (161_104, 0),
            (173_934, 0),
            (186_764, 0),
            (199_595, 0),
            (212_425, 0),
            (225_255, 0),
            (238_086, 0),
            (250_916, 0),
            (263_746, 0),
            (84_189, 0),
            (52_447, 0),
            (46_836, 0),
        ];
        assert_eq!(staked.len(), expected_payouts.len());

        for (p, expected) in staked.into_iter().zip(expected_payouts.into_iter()) {
            let inflation = super::compute_total_payout(
                p * STAKEABLE,
                STAKEABLE,
                TTS,
                IDEAL_STAKE,
                MIN_INFLATION,
                MAX_INFLATION,
                FALLOFF,
                MAX_ROI,
                time_fraction,
            );
            // allowing for some rounding error
            assert_approx_eq!(inflation.0, expected.0, 1);
            assert_approx_eq!(inflation.1, expected.1, 1);
        }
    }
}
