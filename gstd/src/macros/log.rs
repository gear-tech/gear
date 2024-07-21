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
/// Shortcut of [`gstd::msg::log_str`] with formatter.
///
/// # Example
///
/// ```no_run
/// // in program
/// gstd::log!("the answer is {}", 42);
///
/// // on client side, after extracting payload from events.
/// assert_eq!(
///     String::from_utf8_lossy(payload),
///     format!("the answer is 42")
/// );
/// ```
#[macro_export]
macro_rules! log {
    ($($arg:tt)*) => {{
        $crate::msg::log_str($crate::format!($($arg)*))
    }};
}
