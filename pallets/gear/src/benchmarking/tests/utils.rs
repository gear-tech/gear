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

use crate::{BlockGasLimitOf, ProcessStatus, QueueState, SentOf};

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

        let remaining_weight = remaining_weight.unwrap_or_else(BlockGasLimitOf::<T>::get);
        log::debug!(
            "🧱 Running run #{:?} (gear #{:?}) with weight {}",
            SystemPallet::<T>::block_number(),
            Gear::<T>::block_number(),
            remaining_weight
        );

        Gear::<T>::run_queue(remaining_weight);
        Gear::<T>::processing_completed();
        Gear::<T>::on_finalize(SystemPallet::<T>::block_number());

        assert!(!matches!(
            QueueState::<T>::get(),
            ProcessStatus::SkippedOrFailed
        ));
    }
}
