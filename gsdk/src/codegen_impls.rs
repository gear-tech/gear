// This file is part of Gear.
//
// Copyright (C) 2025 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use gear_core::pages::GearPage;

use crate::gear::runtime_types;

impl From<GearPage> for runtime_types::gear_core::pages::Page {
    fn from(page: GearPage) -> Self {
        Self(page.into())
    }
}

impl runtime_types::pallet_balances::types::ExtraFlags {
    pub const DEFAULT: Self = Self(0);
    pub const NEW_LOGIC: Self = Self(0x80000000_00000000_00000000_00000000u128);
}
