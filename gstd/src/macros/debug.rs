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

/// Add a debug message to the log.
///
/// Same as [`gcore::debug`] but uses heap instead of stack for formatting.
#[cfg(any(feature = "debug", debug_assertions))]
#[macro_export]
macro_rules! heap_debug {
    ($fmt:expr) => {
        $crate::ext::debug(&$crate::format!($fmt))
    };
    ($fmt:expr, $($args:tt)*) => {
        $crate::ext::debug(&$crate::format!($fmt, $($args)*))
    };
}

#[cfg(not(any(feature = "debug", debug_assertions)))]
#[allow(missing_docs)]
#[macro_export]
macro_rules! heap_debug {
    ($fmt:expr) => {};
    ($fmt:expr, $($args:tt)*) => {};
}

/// Prints and returns the value of a given expression for quick and dirty
/// debugging.
///
/// Similar to the standard library's
/// [`dbg!`](https://doc.rust-lang.org/std/macro.dbg.html) macro.
#[macro_export]
macro_rules! dbg {
    () => {
        $crate::debug!("[{}:{}:{}]", $crate::prelude::file!(), $crate::prelude::line!(), $crate::prelude::column!())
    };
    ($val:expr $(,)?) => {
        match $val {
            tmp => {
                $crate::debug!("[{}:{}:{}] {} = {:#?}",
                    $crate::prelude::file!(),
                    $crate::prelude::line!(),
                    $crate::prelude::column!(),
                    $crate::prelude::stringify!($val),
                    &tmp,
                );
                tmp
            }
        }
    };
    ($($val:expr),+ $(,)?) => {
        ($($crate::dbg!($val)),+,)
    };
}
