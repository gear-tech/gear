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

use core::marker::PhantomData;

use crate::*;
use frame_support::{
    pallet_prelude::StorageVersion,
    traits::{GetStorageVersion, OnRuntimeUpgrade},
};
use sp_runtime::traits::Get;

pub struct NominationPoolsMigrationV4OldPallet;
impl Get<Perbill> for NominationPoolsMigrationV4OldPallet {
    fn get() -> Perbill {
        Perbill::from_percent(10)
    }
}

pub struct UpdatePalletsVersions<T>(PhantomData<T>);

impl<
        T: pallet_multisig::Config
            + pallet_nomination_pools::Config
            + pallet_bounties::Config
            + pallet_election_provider_multi_phase::Config,
    > OnRuntimeUpgrade for UpdatePalletsVersions<T>
{
    fn on_runtime_upgrade() -> Weight {
        let mut writes = 0;
        // pallet_multisig
        let onchain = pallet_multisig::Pallet::<T>::on_chain_storage_version();
        if onchain == 0 {
            log::info!("pallet_multisig onchain: {:?}", onchain);
            StorageVersion::new(1).put::<pallet_multisig::Pallet<T>>();
            writes += 1;
        }

        let onchain = pallet_multisig::Pallet::<T>::on_chain_storage_version();
        log::info!("pallet_multisig onchain: {:?}", onchain);

        // pallet_nomination_pools
        let onchain = pallet_nomination_pools::Pallet::<T>::on_chain_storage_version();
        if onchain == 0 {
            log::info!("pallet_nomination_pools onchain: {:?}", onchain);
            StorageVersion::new(5).put::<pallet_nomination_pools::Pallet<T>>();
            writes += 1;
        }

        let onchain = pallet_nomination_pools::Pallet::<T>::on_chain_storage_version();
        log::info!("pallet_nomination_pools onchain: {:?}", onchain);

        // pallet_election_provider_multi_phase
        let onchain = pallet_election_provider_multi_phase::Pallet::<T>::on_chain_storage_version();
        if onchain == 0 {
            log::info!(
                "pallet_election_provider_multi_phase onchain: {:?}",
                onchain
            );
            StorageVersion::new(1).put::<pallet_election_provider_multi_phase::Pallet<T>>();
            writes += 1;
        }

        let onchain = pallet_election_provider_multi_phase::Pallet::<T>::on_chain_storage_version();
        log::info!(
            "pallet_election_provider_multi_phase onchain: {:?}",
            onchain
        );

        // pallet_bounties
        StorageVersion::new(4).put::<pallet_bounties::Pallet<T>>();
        writes += 1;

        T::DbWeight::get().reads_writes(6, writes)
    }
}

/// All migrations that will run on the next runtime upgrade.
pub type Migrations = (
    // v1030
    UpdatePalletsVersions<Runtime>,
    pallet_offences::migration::v1::MigrateToV1<Runtime>,
    // v1040
    pallet_im_online::migration::v1::Migration<Runtime>,
    // unreleased
    pallet_gear_program::migrations::MigrateToV3<Runtime>,
);
