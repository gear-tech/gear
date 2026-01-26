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

use ethexe_common::db::{DkgStorageRO, DkgStorageRW, SignStorageRO, SignStorageRW};
use ethexe_db::Database;

/// Storage bound required by the DKG engine.
pub trait DkgStore: DkgStorageRO + DkgStorageRW + Clone {}

impl DkgStore for Database {}

/// Storage bound required by the ROAST engine (DKG + sign session state).
pub trait RoastStore: DkgStorageRO + SignStorageRO + SignStorageRW + Clone {
    fn prune_roast_caches_before(&self, min_era: u64) -> (usize, usize);
}

impl RoastStore for Database {
    fn prune_roast_caches_before(&self, min_era: u64) -> (usize, usize) {
        self.prune_roast_caches_before(min_era)
    }
}
