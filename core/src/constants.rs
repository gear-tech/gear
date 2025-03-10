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

//! Runtime constants which we want to keep in one place instead
//! of changing them in multiple places.

/// Limit of outgoing messages per block.
pub const OUTGOING_LIMIT: u32 = 1024;
/// Outgoing bytes limit per block.
/// 64 MB, must be less than max runtime heap memory.
pub const OUTGOING_BYTES_LIMIT: u32 = 64 * 1024 * 1024;
/// Mailbox threshold default value. This is minimal amount of gas
/// for message to be added to mailbox.
pub const MAILBOX_THRESHOLD: u64 = 3000;
/// Performance multiplier default value.
// TODO(playx): what's the use of this constant?
pub const PERFORMANCE_MULTIPLIER: u32 = 100;
/// Default bank address. It points to the bank pallet.
pub const BANK_ADDRESS: u64 = 15082001;
/// Maximum number of block number to block hash mappings to keep
pub const BLOCK_HASH_COUNT: u64 = 250;
