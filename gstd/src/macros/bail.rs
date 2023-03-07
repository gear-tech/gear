// This file is part of Gear.

// Copyright (C) 2021-2022 Gear Technologies Inc.
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

/// Unwrap `Result<T, E>` to `T` if it is `Ok(T)` or panic with the provided
/// message if the result is `Err(E)`.
///
/// The message argument(s) can be either:
///
/// - a string literal;
/// - two string literals: the first is provided with panic when the program has
///   been compiled in release mode, and the second is provided when the program
///   is compiled in debug mode (either with `--debug` or `--features=debug`
///   parameters);
/// - a format string followed by arguments.
///
/// # Examples
///
/// Unwrap `Ok(i32)` value to `i32`:
///
/// ```
/// use gstd::bail;
///
/// let result: Result<i32, ()> = Ok(42);
/// let value = bail!(result, "Unreachable as `result` is `Ok`");
/// assert_eq!(value, 42);
/// ```
///
/// Panic when trying to unwrap the `Err(&str)` value:
///
/// ```should_panic
/// # use gstd::bail;
/// let result: Result<(), &str> = Err("Wrong value");
/// // The next line will result in panic
/// let value = bail!(result, "We have an error value");
/// ```
///
/// Panic with different messages for release and debug profiles:
///
/// ```should_panic
/// # use gstd::bail;
/// let result: Result<(), &str> = Err("Wrong value");
/// // The next line will result in panic
/// let value = bail!(result, "Message in release mode", "Message in debug mode");
/// ```
///
/// Panic with the formatted message string:
///
/// ```should_panic
/// # use gstd::bail;
/// let result: Result<(), &str> = Err("Wrong value");
/// let a = 42;
/// // The next line will result in panic
/// let value = bail!(result, "Error", "a = {}", a);
/// ```
#[cfg(any(feature = "debug", debug_assertions))]
#[macro_export]
macro_rules! bail {
    ($res:expr, $msg:literal) => {
        $res.expect($msg)
    };
    ($res:expr, $expl:literal, $fmtd:literal) => {
        {
            let _ = $expl;
            $res.expect($fmtd)
        }
    };
    ($res:expr, $expl:literal, $fmt:literal $(, $args:tt)+) => {
        {
            let _ = $expl;
            $res.expect(&$crate::prelude::format!($fmt $(, $args)+))
        }
    };
}

#[cfg(not(feature = "debug"))]
#[cfg(not(debug_assertions))]
#[allow(missing_docs)]
#[macro_export]
macro_rules! bail {
    ($res:expr, $msg:literal) => {
        match $res {
            Ok(v) => v,
            Err(_) => $crate::prelude::panic!($msg),
        }
    };
    ($res:expr, $expl:literal, $fmtd:literal) => {
        let _ = $fmtd;
        match $res {
            Ok(v) => v,
            Err(_) => $crate::prelude::panic!($expl),
        }
    };
    ($res:expr, $expl:literal, $fmt:literal $(, $args:tt)+) => {
        let _ = $fmt;
        $(let _ = $args;) +
        match $res {
            Ok(v) => v,
            Err(_) => $crate::prelude::panic!($expl),
        }
    };
}
