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

use runtime_primitives::BlockNumber;

/// Vara SS58 Prefix
pub const VARA_SS58PREFIX: u8 = 137;

/// Vara Decimals
pub const VARA_DECIMAL: u8 = 12;

/// Vara Token Symbol
pub const VARA_TOKEN_SYMBOL: &str = "VARA";

/// The minimal amount of blocks to resume represented as a factor of weeks.
pub const RENT_RESUME_WEEK_FACTOR: BlockNumber = 4;

/// The amount of blocks for processing resume session represented as a factor of hours.
pub const RESUME_SESSION_DURATION_HOUR_FACTOR: BlockNumber = 1;
