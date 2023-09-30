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

use frame_support::{
    pallet_prelude::Weight,
    traits::{tokens::Imbalance, Currency, OnRuntimeUpgrade},
};
use frame_system::Config;

use sp_runtime::traits::{Get, Zero};

use crate::*;

pub type Migrations = MigrateDustTreasury<Runtime>;

const TARGET_TOTAL_ISSUANCE: u128 = 10_000_000_000_000_000_000_000;

pub struct MigrateDustTreasury<T: Config + pallet_balances::Config + pallet_treasury::Config>(
    sp_std::marker::PhantomData<T>,
);

impl<T: frame_system::Config + pallet_balances::Config + pallet_treasury::Config> OnRuntimeUpgrade
    for MigrateDustTreasury<T>
{
    #[cfg(feature = "try-runtime")]
    fn pre_upgrade() -> Result<Vec<u8>, &'static str> {
        let total_issuance = Balances::total_issuance();
        log::info!("total issuance:  {total_issuance:?}");
        log::info!("target issuance:  {TARGET_TOTAL_ISSUANCE:?}");
        let diff = TARGET_TOTAL_ISSUANCE.saturating_sub(total_issuance);
        log::info!("difference:  {diff:?}");
        Ok(Default::default())
    }

    fn on_runtime_upgrade() -> Weight {
        let version = T::Version::get().spec_version;

        let total_issuance = Balances::total_issuance();

        log::info!("🚚 Running dust to treasury migration with current spec version {version:?}");

        if version <= 1010 && total_issuance < TARGET_TOTAL_ISSUANCE {
            let issuance_diff = TARGET_TOTAL_ISSUANCE.saturating_sub(total_issuance);
            let treasury_account = Treasury::account_id();

            let positive_imbalance = <Balances as Currency<AccountId>>::deposit_creating(
                &treasury_account,
                issuance_diff,
            );

            if positive_imbalance.peek() == issuance_diff {
                log::info!("Ok");
            } else {
                log::info!(
                    "Something went wrong: positive_imbalance - {:?}, issuance_diff - {:?}, {}",
                    positive_imbalance.peek(),
                    issuance_diff,
                    positive_imbalance.peek() == issuance_diff,
                )
            }
            T::DbWeight::get().writes(2)
        } else {
            log::info!(
                "❌ Migration dust to treasury did not execute. This probably should be removed"
            );
            Zero::zero()
        }
    }

    #[cfg(feature = "try-runtime")]
    fn post_upgrade(_state: Vec<u8>) -> Result<(), &'static str> {
        log::info!("Runtime successfully migrated.");
        log::info!(
            "Total issuance == target issuance: {}",
            Balances::total_issuance() == TARGET_TOTAL_ISSUANCE
        );
        Ok(())
    }
}
