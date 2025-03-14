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

// # Messaging constants

/// The maximum amount of messages that can be produced in during all message executions.
pub const OUTGOING_LIMIT: u32 = 1024;

/// The maximum amount of bytes in outgoing messages during message execution.
/// 64 MB, must be less than max runtime heap memory.
pub const OUTGOING_BYTES_LIMIT: u32 = 64 * 1024 * 1024;

/// The minimal gas amount for message to be inserted in mailbox.
///
/// This gas will be consuming as rent for storing and message will be available
/// for reply or claim, once gas ends, message removes.
///
/// Messages with gas limit less than that minimum will not be added in mailbox,
/// but will be seen in events.
pub const MAILBOX_THRESHOLD: u64 = 3000;

// # Runtime constants

/// Performance multiplier default value.
pub const PERFORMANCE_MULTIPLIER: u32 = 100;
