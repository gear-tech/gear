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

use gcore::{errors::Result, ActorId, MessageId};

/// Prints a string to the log.
///
/// For the internal logic, this macro sends messages to empty program
/// which results as logging, the sent logs can be extracted from the
/// chain event `pallet_gear::Event::UserMessageSent`.
///
/// ```no_run
/// let GearEvent::UserMessageSent {
///   message: UserMessage {
///     // The payload here is the log you sent with the method.
///     payload,
///     destination: ActorId::zero(),
///     ...
///   },
///   ...
/// } = event;
/// ```
///
/// # Example
///
/// ```no_run
/// // program side
/// let log = "the answer is 42";
/// gstd::msg::log_str(log);
///
/// // client side, after extracting payload from events.
/// assert_eq!(String::from_utf8_lossy(payload), log.into());
/// ```
pub fn log_str(s: impl AsRef<str>) -> Result<MessageId> {
    log(s.as_ref().as_bytes())
}

/// Log raw bytes.
///
/// Similar to [`log_str`], but without any encoding.
///
/// # Example
///
/// ```no_run
/// // program side
/// gstd::msg::log(b"42");
///
/// // client side, after extracting payload from events.
/// assert_eq!(payload, b"42".into());
/// ```
pub fn log(data: impl AsRef<[u8]>) -> Result<MessageId> {
    crate::msg::send_bytes(ActorId::zero(), data.as_ref(), 0)
}
