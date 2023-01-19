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

use super::*;

use crate::{GasAllowanceOf, SentOf};
use frame_system::limits::BlockWeights;

pub fn default_account<T: Origin>() -> T {
    benchmarking::account::<T>("default", 0, 0)
}

#[cfg(feature = "std")]
pub fn init_logger() {
    let _ = env_logger::Builder::from_default_env()
        .format_module_path(false)
        .format_level(true)
        .try_init();
}

/// Gets next message id, but doesn't remain changed the state of the nonces
pub fn get_next_message_id<T>(user_id: impl Origin) -> MessageId
where
    T: Config,
    T::AccountId: Origin,
{
    let ret_id = Gear::<T>::next_message_id(user_id.into_origin());
    SentOf::<T>::decrease();
    ret_id
}

pub fn run_to_next_block<T: Config>(remaining_weight: Option<u64>)
where
    T::AccountId: Origin,
{
    let current_block: u32 = SystemPallet::<T>::block_number().unique_saturated_into();
    run_to_block::<T>(current_block + 1, remaining_weight);
}

pub fn run_to_block<T: Config>(n: u32, remaining_weight: Option<u64>)
where
    T::AccountId: Origin,
{
    while SystemPallet::<T>::block_number() < n.unique_saturated_into() {
        SystemPallet::<T>::on_finalize(SystemPallet::<T>::block_number());

        init_block::<T>(Some(SystemPallet::<T>::block_number()));

        Gear::<T>::on_initialize(SystemPallet::<T>::block_number());

        if let Some(remaining_weight) = remaining_weight {
            GasAllowanceOf::<T>::put(remaining_weight);
            let max_block_weight =
                <<T as frame_system::Config>::BlockWeights as Get<BlockWeights>>::get().max_block;
            SystemPallet::<T>::register_extra_weight_unchecked(
                max_block_weight.saturating_sub(frame_support::weights::Weight::from_ref_time(
                    remaining_weight,
                )),
                frame_support::dispatch::DispatchClass::Normal,
            );
        }

        Gear::<T>::run(frame_support::dispatch::RawOrigin::None.into()).unwrap();
        Gear::<T>::on_finalize(SystemPallet::<T>::block_number());
    }
}
