// This file is part of Gear.

// Copyright (C) 2021-2024 Gear Technologies Inc.
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

/// All migrations that will run on the next runtime upgrade.
pub type Migrations = (
    /* release v1.2.0 */
    // migration for stored contexts
    pallet_gear_messenger::migrations::MigrateToV3<Runtime>,
    // check for existence of the rent pool account
    pallet_gear_staking_rewards::migrations::CheckRentPoolId<Runtime>,
    /* unapplied migrations */
    pallet_gear_program::migration::AppendStackEndMigration<Runtime>,
    /* not yet executed */
    // migration for removed waiting init list
    pallet_gear::migrations::MigrateWaitingInitList<Runtime>,
);
