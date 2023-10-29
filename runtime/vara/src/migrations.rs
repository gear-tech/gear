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

use crate::*;
use sp_runtime::traits::Get;

pub struct NominationPoolsMigrationV4OldPallet;
impl Get<Perbill> for NominationPoolsMigrationV4OldPallet {
    fn get() -> Perbill {
        Perbill::from_percent(10)
    }
}

/// All migrations that will run on the next runtime upgrade.
///
/// Should be cleared after every release.
pub type Migrations = (
    // unreleased
    pallet_nomination_pools::migration::v4::MigrateV3ToV5<
        Runtime,
        NominationPoolsMigrationV4OldPallet,
    >,
    pallet_offences::migration::v1::MigrateToV1<Runtime>,
    pallet_gear_program::migrations::MigrateToV3<Runtime>,
);
