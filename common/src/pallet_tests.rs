// This file is part of Gear.

// Copyright (C) 2023 Gear Technologies Inc.
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

/// Module contains macros that help to implement Config type
/// for various pallets of Substrate.
/// All used types should be in scope.

#[macro_export]
macro_rules! impl_pallet_balances {
    ($runtime:ty) => {
        impl pallet_balances::Config for $runtime {
            type MaxLocks = ();
            type MaxReserves = ();
            type ReserveIdentifier = [u8; 8];
            type Balance = Balance;
            type DustRemoval = ();
            type RuntimeEvent = RuntimeEvent;
            type ExistentialDeposit = ExistentialDeposit;
            type AccountStore = System;
            type WeightInfo = ();
        }
    };
}
