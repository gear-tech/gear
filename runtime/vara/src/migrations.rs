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

use crate::*;

/// All migrations that will run on the next runtime upgrade.
pub type Migrations = (
    // Migrate Identity pallet for Usernames
    pallet_identity::migration::versioned::V0ToV1<Runtime, { u64::MAX }>,
    pallet_staking::migrations::v15::MigrateV14ToV15<Runtime>,
    pallet_nomination_pools::migration::versioned::V7ToV8<Runtime>,
    CleanupFellowshipIndex<Runtime>,
);

pub struct CleanupFellowshipIndex<
    T: pallet_ranked_collective::Config<governance::FellowshipCollectiveInstance>,
>(core::marker::PhantomData<T>);

impl<T: pallet_ranked_collective::Config<governance::FellowshipCollectiveInstance>>
    frame_support::traits::OnRuntimeUpgrade for CleanupFellowshipIndex<T>
{
    fn on_runtime_upgrade() -> Weight {
        use pallet_ranked_collective::{IdToIndex, Members};
        use sp_core::Get;

        let mut weight = Weight::zero();

        IdToIndex::<T, governance::FellowshipCollectiveInstance>::iter_prefix(0).for_each(
            |(who, _member_index)| {
                weight = weight.saturating_add(T::DbWeight::get().reads(1));

                if !Members::<T, governance::FellowshipCollectiveInstance>::contains_key(&who) {
                    log::debug!("Removing {who:?} from index");
                    weight = weight.saturating_add(T::DbWeight::get().writes(1));

                    IdToIndex::<T, governance::FellowshipCollectiveInstance>::remove(0, who);
                }
            },
        );

        weight
    }
}
