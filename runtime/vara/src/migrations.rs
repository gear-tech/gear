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

/// All migrations that will run on the next runtime upgrade.
use crate::*;

pub type Migrations = (
    // move metadata into attribution
    pallet_gear_program::migrations::v11_code_metadata_delete_migration::MigrateRemoveCodeMetadata<Runtime>,
    // migrate program code hash to code id and remove code_exports and static_pages
    pallet_gear_program::migrations::v12_program_code_id_migration::MigrateProgramCodeHashToCodeId<Runtime>,
    // split instrumented code into separate storage items
    pallet_gear_program::migrations::v13_split_instrumented_code_migration::MigrateSplitInstrumentedCode<Runtime>,
);
