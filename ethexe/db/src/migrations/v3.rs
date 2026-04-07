// This file is part of Gear.
//
// Copyright (C) 2026 Gear Technologies Inc.
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

use super::InitConfig;
use anyhow::Result;

use crate::RawDatabase;

pub const VERSION: u32 = 3;

pub async fn migration_from_v2(_: &InitConfig, _: &RawDatabase) -> Result<()> {
    // TODO: `Announce` keeps the same hash, but its stored value format changed:
    // `Vec<SignedInjectedTransaction>` was replaced with
    // `Vec<AnnounceInjectedTransaction>`.
    //
    // The migration must:
    // 1. Read announces using the previous schema.
    // 2. Backfill injected transaction storage with full signed transactions.
    // 3. Rewrite announces into the new compact schema.
    // 4. Verify `Announce::to_hash()` remains unchanged for migrated values.
    todo!()
}
