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

use gear_core::ids::ProgramId;
use gsdk::ext::sp_runtime::AccountId32;

/// A trait for convenient conversion into Substrate's AccountId32.
pub trait IntoAccountId32 {
    fn into_account_id(self) -> AccountId32;
}

impl IntoAccountId32 for AccountId32 {
    fn into_account_id(self) -> AccountId32 {
        self
    }
}

impl IntoAccountId32 for &AccountId32 {
    fn into_account_id(self) -> AccountId32 {
        self.clone()
    }
}

impl IntoAccountId32 for ProgramId {
    fn into_account_id(self) -> AccountId32 {
        AccountId32::new(self.into_bytes())
    }
}

impl IntoAccountId32 for &ProgramId {
    fn into_account_id(self) -> AccountId32 {
        AccountId32::new(self.into_bytes())
    }
}
