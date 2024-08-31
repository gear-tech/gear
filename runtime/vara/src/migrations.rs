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
    // migration for added section sizes
    pallet_gear_program::migrations::add_section_sizes::AddSectionSizesMigration<Runtime>,
    // substrate v1.4.0
    staking::MigrateToV14<Runtime>,
    pallet_grandpa::migrations::MigrateV4ToV5<Runtime>,
    // move allocations to a separate storage item and remove pages_with_data field from program
    pallet_gear_program::migrations::allocations::MigrateAllocations<Runtime>,
    // SECURITY: DELETE THIS MIGRATION ONCE PERFORMED ON CHAIN.
    staking_exposure_size_dump::MigrateToLowerMaxExposureSize<Runtime>,
);

mod staking {
    use frame_support::{
        pallet_prelude::Weight,
        traits::{GetStorageVersion, OnRuntimeUpgrade},
    };
    use pallet_staking::*;
    use sp_core::Get;

    #[cfg(feature = "try-runtime")]
    use sp_std::vec::Vec;

    #[cfg(feature = "try-runtime")]
    use sp_runtime::TryRuntimeError;

    pub struct MigrateToV14<T>(sp_std::marker::PhantomData<T>);
    impl<T: Config> OnRuntimeUpgrade for MigrateToV14<T> {
        fn on_runtime_upgrade() -> Weight {
            let current = Pallet::<T>::current_storage_version();
            let on_chain = Pallet::<T>::on_chain_storage_version();

            if current == 14 && on_chain == 13 {
                current.put::<Pallet<T>>();

                log::info!("v14 applied successfully.");
                T::DbWeight::get().reads_writes(1, 1)
            } else {
                log::warn!("v14 not applied.");
                T::DbWeight::get().reads(1)
            }
        }

        #[cfg(feature = "try-runtime")]
        fn pre_upgrade() -> Result<Vec<u8>, TryRuntimeError> {
            Ok(Default::default())
        }

        #[cfg(feature = "try-runtime")]
        fn post_upgrade(_state: Vec<u8>) -> Result<(), TryRuntimeError> {
            frame_support::ensure!(
                Pallet::<T>::on_chain_storage_version() == 14,
                "v14 not applied"
            );
            Ok(())
        }
    }
}

mod staking_exposure_size_dump {
    use frame_support::{
        defensive_assert,
        pallet_prelude::Weight,
        traits::{Defensive, DefensiveSaturating, OnRuntimeUpgrade},
    };
    use pallet_staking::*;
    use sp_core::Get;
    use sp_staking::{EraIndex, Page, PagedExposureMetadata};

    // SECURITY: DELETE THIS MIGRATION ONCE PERFORMED ON CHAIN.
    pub struct MigrateToLowerMaxExposureSize<T>(sp_std::marker::PhantomData<T>);
    impl<T: Config> OnRuntimeUpgrade for MigrateToLowerMaxExposureSize<T> {
        fn on_runtime_upgrade() -> Weight {
            const OLD_MAX_EXPOSURE_PAGE_SIZE: u32 = 512;

            let mut weight = Weight::zero();

            // Per each full page of OLD_MAX_EXPOSURE_PAGE_SIZE there's 2 new pages.
            debug_assert_eq!(T::MaxExposurePageSize::get(), 256);

            for (era, validator, overview) in <ErasStakersOverview<T>>::drain() {
                log::debug!("Migrating validator {validator:?} for era {era:?}");
                log::debug!("1. Overview removed");
                weight = weight.saturating_add(T::DbWeight::get().writes(1));

                let page_count = overview.page_count;
                let nominator_count = overview.nominator_count;

                let exposure = take_full_paged_exposure::<T>(era, &validator, overview);
                log::debug!("2. Paged exposures removed");
                weight = weight.saturating_add(T::DbWeight::get().writes(1 + page_count as u64));

                let new_overview = set_exposure::<T>(era, &validator, exposure);
                log::debug!("3. Overview and paged exposures set");
                weight = weight
                    .saturating_add(T::DbWeight::get().writes(1 + new_overview.page_count as u64));

                // Pages are chunks with size of OLD_MAX_EXPOSURE_PAGE_SIZE.
                // At this point, ClaimedRewards will be migrated as:
                // [0, 1, 3] => [(0, 1), (2, 3), (6, 7)]
                // NOTE: pages start from 0
                // NOTE: new No7 will exist if old No3 has len > new MAX_EXPOSURE_PAGE_SIZE */
                let last_chunk_size =
                    nominator_count - OLD_MAX_EXPOSURE_PAGE_SIZE * page_count.saturating_sub(1);
                let split_last_page = last_chunk_size > T::MaxExposurePageSize::get();

                let claimed_rewards = <ClaimedRewards<T>>::take(era, &validator);

                if !claimed_rewards.is_empty() {
                    log::debug!("4. Claimed rewards removed");
                    let new_claimed_rewards =
                        map_claimed_rewards(claimed_rewards, page_count, split_last_page);

                    if !new_claimed_rewards.is_empty() {
                        <ClaimedRewards<T>>::insert(era, &validator, new_claimed_rewards);
                        log::debug!(
                            "5. Claimed rewards set (last page splitted = {split_last_page}"
                        );
                        weight = weight.saturating_add(T::DbWeight::get().writes(1));
                    }
                } else {
                    log::debug!("4. Claimed rewards are empty");
                    weight = weight.saturating_add(T::DbWeight::get().reads(1));
                }
            }

            weight
        }
    }

    // Claimed is non-empty Vec.
    pub(crate) fn map_claimed_rewards(
        mut claimed: Vec<u32>,
        page_count: u32,
        split_last_page: bool,
    ) -> Vec<u32> {
        let Some(last_page_claimed) = claimed.pop() else {
            // Nothing to map
            return Default::default();
        };

        let Some(last_page) = page_count.checked_sub(1) else {
            // No pages at all, so none could be claimed: nothing to map.
            return Default::default();
        };

        let mut new_claimed = Vec::with_capacity(claimed.len() * 2);

        for page in claimed {
            new_claimed.push(page * 2);
            new_claimed.push(page * 2 + 1);
        }

        new_claimed.push(last_page_claimed * 2);

        if last_page_claimed != last_page || split_last_page {
            new_claimed.push(last_page_claimed * 2 + 1);
        }

        new_claimed
    }

    // NOTE: FULLY copy-pasted from substrate code of `EraInfo` impl.
    /// Store exposure for elected validators at start of an era.
    pub(crate) fn set_exposure<T: Config>(
        era: EraIndex,
        validator: &T::AccountId,
        exposure: Exposure<T::AccountId, BalanceOf<T>>,
    ) -> PagedExposureMetadata<BalanceOf<T>> {
        let page_size = T::MaxExposurePageSize::get();

        let nominator_count = exposure.others.len();
        // expected page count is the number of nominators divided by the page size, rounded up.
        let expected_page_count =
            nominator_count.defensive_saturating_add(page_size as usize - 1) / page_size as usize;

        let (exposure_metadata, exposure_pages) = exposure.into_pages(page_size);
        defensive_assert!(
            exposure_pages.len() == expected_page_count,
            "unexpected page count"
        );

        <ErasStakersOverview<T>>::insert(era, validator, &exposure_metadata);
        exposure_pages
            .iter()
            .enumerate()
            .for_each(|(page, paged_exposure)| {
                <ErasStakersPaged<T>>::insert((era, validator, page as Page), &paged_exposure);
            });

        exposure_metadata
    }

    // NOTE: PARTIALLY copy-pasted from substrate code of `EraInfo` impl.
    /// Returns None in case of non-paged stake (legacy approach == nothing to do).
    pub(crate) fn take_full_paged_exposure<T: Config>(
        era: EraIndex,
        validator: &T::AccountId,
        overview: PagedExposureMetadata<BalanceOf<T>>,
    ) -> Exposure<T::AccountId, BalanceOf<T>> {
        let mut others = Vec::with_capacity(overview.nominator_count as usize);
        for page in 0..overview.page_count {
            let nominators = <ErasStakersPaged<T>>::take((era, validator, page));
            others.append(&mut nominators.map(|n| n.others).defensive_unwrap_or_default());
        }

        Exposure {
            total: overview.total,
            own: overview.own,
            others,
        }
    }
}
