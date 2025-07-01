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

//! Builtin actor pallet tests.

use gear_core::primitives::ActorId;

mod bad_builtin_ids;
mod basic;
mod bls381;
mod proxy;
mod staking;

pub(crate) fn get_last_program_id<T: frame_system::Config + pallet_gear::Config>() -> ActorId
where
    <T as frame_system::Config>::RuntimeEvent: TryInto<pallet_gear::Event<T>>,
{
    frame_system::Pallet::<T>::events()
        .iter()
        .rev()
        .find_map(|e| {
            e.event
                .clone()
                .try_into()
                .map(|e| {
                    if let pallet_gear::Event::<T>::ProgramChanged { id, .. } = e {
                        Some(id)
                    } else {
                        None
                    }
                })
                .ok()
                .flatten()
        })
        .expect("can't find program creation event")
}
