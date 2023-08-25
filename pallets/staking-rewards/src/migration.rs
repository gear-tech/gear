// This file is part of Gear.

// Copyright (C) 2022-2023 Gear Technologies Inc.
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

use crate::{pallet, Config, Pallet, Weight};
use frame_support::traits::{Get, GetStorageVersion, OnRuntimeUpgrade};
use sp_runtime::Perquintill;
use sp_std::marker::PhantomData;

pub struct MigrateToV2<T: Config>(PhantomData<T>);

impl<T: Config> OnRuntimeUpgrade for MigrateToV2<T> {
    fn on_runtime_upgrade() -> Weight {
        let current = Pallet::<T>::current_storage_version();
        let onchain = Pallet::<T>::on_chain_storage_version();

        log::info!(
            "🚚 Running migration with current storage version {:?} / onchain {:?}",
            current,
            onchain
        );

        let mut weight = T::DbWeight::get().reads(1); // 1 read for on chain storage version.

        if current == 2 && onchain == 1 {
            // Adjusted target inflation parameter: 6.00%
            let adjusted_inflation: Perquintill = Perquintill::from_percent(6);
            pallet::TargetInflation::<T>::put(adjusted_inflation);

            current.put::<Pallet<T>>();

            log::info!("Successfully migrated storage from v1 to v2");

            // 1 write for `TargetInflation`
            weight += T::DbWeight::get().writes(1)
        } else {
            log::info!("❌ Migration did not execute. This probably should be removed");
        }

        weight
    }

    #[cfg(feature = "try-runtime")]
    fn pre_upgrade() -> Result<Vec<u8>, &'static str> {
        use parity_scale_codec::Encode;

        let inflation = pallet::TargetInflation::<T>::get();
        assert_eq!(inflation, Perquintill::from_rational(578_u64, 10_000_u64));
        Ok(inflation.encode())
    }

    #[cfg(feature = "try-runtime")]
    fn post_upgrade(state: Vec<u8>) -> Result<(), &'static str> {
        use parity_scale_codec::Decode;

        let inflation: Perquintill = Decode::decode(&mut &state[..]).unwrap();
        assert_eq!(inflation, Perquintill::from_percent(6),);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::*;
    use frame_support::traits::StorageVersion;

    #[test]
    fn migrate_to_v2() {
        ExtBuilder::default()
            .initial_authorities(vec![(VAL_1_STASH, VAL_1_CONTROLLER, VAL_1_AUTH_ID)])
            .stash(VALIDATOR_STAKE)
            .endowment(ENDOWMENT)
            .target_inflation(Perquintill::from_rational(578_u64, 10_000_u64))
            .build()
            .execute_with(|| {
                StorageVersion::new(1).put::<Pallet<Test>>();

                let weight = MigrateToV2::<Test>::on_runtime_upgrade();
                assert_ne!(weight.ref_time(), 0);

                assert_eq!(
                    pallet::TargetInflation::<Test>::get(),
                    Perquintill::from_percent(6),
                );
            })
    }
}
