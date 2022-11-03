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

//! Gear `bail!` macro. Provides custom unwrapping realization.

/// **The `bail!` macro**
///
/// Unwraps `Result<T, E: Debug>`.
/// In case of argument value Ok(T) returns T, else panics with custom message.
///
/// The macro has three implementations and its behavior depends on the build
/// type: is the `--features=debug` flag added.
///
/// - `bail!(res: Result<T, E: Debug>, msg: &str)`
///
/// Panics with the same `msg` in both modes. Contains error message in debug
/// mode.
///
/// - `bail!(res: Result<T, E: Debug>, static_release: &str, static_debug:
///   &str)`
///
/// Panics with the same `static_release` in release mode and with `static
/// debug` + error message in debug mode.
///
/// - `bail!(res: Result<T, E: Debug>, static_release: &str, formatter: &str,
///   args)`
///
/// Panics with the same `static_release` in release mode and with
/// `format!(formatter, args)` + error message in debug mode.
#[cfg(any(feature = "debug", debug_assertions))]
#[macro_export]
macro_rules! bail {
    ($res:expr, $msg:literal) => {
        $res.expect($msg);
    };
    ($res:expr, $expl:literal, $fmtd:literal) => {
        let _ = $expl;
        $res.expect($fmtd);
    };
    ($res:expr, $expl:literal, $fmt:literal $(, $args:tt)+) => {
        let _ = $expl;
        $res.expect(&$crate::prelude::format!($fmt $(, $args)+));
    };
}

#[cfg(not(feature = "debug"))]
#[cfg(not(debug_assertions))]
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
