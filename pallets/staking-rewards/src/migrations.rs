// This file is part of Gear.

// Copyright (C) Gear Technologies Inc.
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

use crate::{Config, Pallet};
use frame_support::traits::{Currency, OnRuntimeUpgrade};
use sp_std::marker::PhantomData;
#[cfg(feature = "try-runtime")]
use {sp_runtime::TryRuntimeError, sp_std::vec::Vec};

pub struct CheckRentPoolId<T: Config>(PhantomData<T>);

impl<T: Config> OnRuntimeUpgrade for CheckRentPoolId<T> {
    #[cfg(feature = "try-runtime")]
    fn pre_upgrade() -> Result<Vec<u8>, TryRuntimeError> {
        log::info!("🚚 Running migration check");

        if T::Currency::total_balance(&Pallet::<T>::rent_pool_account_id())
            < T::Currency::minimum_balance()
        {
            return Err(TryRuntimeError::Other("Rent pool account does not exist!"));
        }

        Ok(Default::default())
    }
}
