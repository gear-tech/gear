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

//! Schema version 5 anchor.
//!
//! Holds the [`VERSION`] constant referenced by
//! [`super::OLDEST_SUPPORTED_VERSION`] and [`super::LATEST_VERSION`]. No
//! migration function lives here — when the next schema bump lands, the
//! new module (e.g. `v6`) gets a `migration_from_v5` and the
//! [`super::MIGRATIONS`] slice grows.

pub const VERSION: u32 = 5;
