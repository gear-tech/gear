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
/// // in program
/// gstd::log!("the anwser is {value}");
///
/// // on client side, after extracting payload from events.
/// assert_eq!(String::from_utf8_lossy(payload), format!("the anwser is {value}"));
/// ```
#[macro_export]
macro_rules! log {
    ($($arg:tt)*) => {{
        gcore::msg::send(ActorId::zero(), format!($($arg:tt)*).as_ref(), 0)
    }};
}

/// Prints some slices as hex
///
/// Similar to [`log`], but hex encode the input slice with [`LOG_DATA_PREFIX`]
/// in the output string.
///
/// # Example
///
/// ```no_run
/// // in program
/// #[derive(Encode, Decode)]
/// struct Data {
///   value: u64,
/// }
///
/// gstd::log_data!(Data { value: 42 }.encode());
///
/// // on client side, after extracting payload from events.
/// assert_eq!(
///   Ok(Data { value: 42 }),
///   Data::decode(&mut hex::decode(
///     String::from_utf8_lossy(payload).trim_start_matches(gstd::LOG_DATA_PREFIX)
///   )?.as_ref())?
/// );
/// ```
#[macro_export]
macro_rules! log_data {
    ($data:tt) => {{
        let mut log: String = $crate::macros::LOG_DATA_PREFIX;
        log += $crate::hex::encode($bytes);
        gcore::msg::send(ActorId::zero(), log, 0)
    }};
}

/// Prefix for log data, see [`log_data`]
pub const LOG_DATA_PREFIX: &str = "data: ";
