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

//! This module exposes a function which returns the total payout for the era given
//! the era duration and the staking rate.
//! The staking rate in NPoS is the total amount of tokens staked by nominators and validators,
//! divided by the total STAKEABLE tokens amount (as opposed to the total token supply).

use pallet_staking_reward_fn::compute_inflation;
use sp_runtime::{
    FixedU128, PerThing, Perquintill, Saturating,
    traits::{AtLeast32BitUnsigned, CheckedDiv},
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
    static STAKEABLE: u128 = 4_849_000_000; // 48.49% of TTS
    static IDEAL_STAKE: Perquintill = Perquintill::from_percent(85);
    static MIN_INFLATION: Perquintill = Perquintill::from_percent(1);
    static MAX_INFLATION: Perquintill = Perquintill::from_parts(60_000_000_000_000_000_u64); // 6.00%
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
            (72_735_000, 56_676_765),
            (145_470_000, 13_353_529),
            (188_235_294, 0),
            (217_647_059, 0),
            (247_058_824, 0),
            (276_470_588, 0),
            (305_882_353, 0),
            (335_294_118, 0),
            (364_705_882, 0),
            (394_117_647, 0),
            (423_529_412, 0),
            (452_941_176, 0),
            (482_352_941, 0),
            (511_764_706, 0),
            (541_176_471, 0),
            (570_588_235, 0),
            (600_000_000, 0),
            (188_388_348, 0),
            (115_625_000, 0),
            (102_762_136, 0),
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
            (199_138, 155_172),
            (398_275, 36_560),
            (515_360, 0),
            (595_885, 0),
            (676_410, 0),
            (756_935, 0),
            (837_460, 0),
            (917_985, 0),
            (998_510, 0),
            (1_079_035, 0),
            (1_159_560, 0),
            (1_240_085, 0),
            (1_320_610, 0),
            (1_401_135, 0),
            (1_481_660, 0),
            (1_562_185, 0),
            (1_642_710, 0),
            (515_779, 0),
            (316_564, 0),
            (281_347, 0),
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
            (0, 45_630),
            (33_189, 25_862),
            (66_379, 6_093),
            (85_893, 0),
            (99_314, 0),
            (112_735, 0),
            (126_155, 0),
            (139_576, 0),
            (152_997, 0),
            (166_418, 0),
            (179_839, 0),
            (193_260, 0),
            (206_680, 0),
            (220_101, 0),
            (233_522, 0),
            (246_943, 0),
            (260_364, 0),
            (273_785, 0),
            (85_963, 0),
            (52_760, 0),
            (46_891, 0),
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
