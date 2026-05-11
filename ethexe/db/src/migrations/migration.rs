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

//! Per-step database migration trait.
//!
//! Implementations live next to the version they upgrade *from* — e.g.
//! `v1::migration_from_v0` produces a v1 database from a v0 one. The
//! driver in [`super::migrate`] walks [`super::MIGRATIONS`] in order,
//! applying each step whose `source_version` matches the on-disk one.

use super::InitConfig;
use crate::RawDatabase;
use anyhow::Result;
use std::pin::Pin;

/// A single schema upgrade step. Implementations must be idempotent on
/// the migration's target version: running the same migration twice
/// must not corrupt a database that's already at
/// `source_version + 1`.
pub trait Migration: Sync {
    /// Schema version this migration upgrades from. Successful
    /// application leaves the database at `source_version() + 1`.
    fn source_version(&self) -> u32;

    /// Apply the migration in-place.
    fn migrate<'a>(
        &'a self,
        config: &'a InitConfig,
        db: &'a RawDatabase,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + 'a>>;
}
