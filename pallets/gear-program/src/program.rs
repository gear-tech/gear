// This file is part of Gear.

// Copyright (C) 2022 Gear Technologies Inc.
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
use common::Origin as _;
use gear_core::ids::ProgramId;

impl<T: Config> pallet::Pallet<T> {
    pub fn program_exists(program_id: ProgramId) -> bool {
        common::program_exists(program_id.into_origin()) | Self::program_paused(program_id)
    }

    pub fn reset_storage() {
        PausedPrograms::<T>::clear(u32::MAX, None);
    }
}
