// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use crate::gear::runtime_types;
use gear_core::pages::GearPage;

impl From<GearPage> for runtime_types::gear_core::pages::Page {
    fn from(page: GearPage) -> Self {
        Self(page.into())
    }
}

impl runtime_types::pallet_balances::types::ExtraFlags {
    pub const DEFAULT: Self = Self(0);
    pub const NEW_LOGIC: Self = Self(0x80000000_00000000_00000000_00000000u128);
}
