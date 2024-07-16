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
/// Note that debug messages are available only if the program is compiled
/// in debug mode.
///
/// ```shell
/// cargo build --debug
/// cargo build --features=debug
/// ```
///
/// You can see the debug messages when running the program using the `gtest`
/// crate. To see these messages when executing the program on the node, you
/// should run the node with the `RUST_LOG="gwasm=debug"` environment variable.
///
/// # Examples
///
/// ```
/// use gstd::debug;
///
/// #[no_mangle]
/// extern "C" fn handle() {
///     debug!("String literal");
///
///     let value = 42;
///     debug!("{value}");
///
///     debug!("Formatted: value = {value}");
/// }
/// ```
#[cfg(any(feature = "debug", debug_assertions))]
#[macro_export]
macro_rules! debug {
    ($($arg:tt)*) => {
        $crate::ext::debug(&$crate::format!($($arg)*)).unwrap()
    };
}

#[cfg(not(any(feature = "debug", debug_assertions)))]
#[allow(missing_docs)]
#[macro_export]
macro_rules! debug {
    ($($arg:tt)*) => {};
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
