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

//! Gear `debug!` macro.
//! Enables output of the logs from Wasm if the `debug` feature is enabled.

#[cfg(any(feature = "debug", debug_assertions))]
#[macro_export]
macro_rules! debug {
    ($arg:literal) => {
        $crate::ext::debug(&$crate::prelude::format!("{}", $arg)).unwrap()
    };
    ($arg:expr) => {
        $crate::ext::debug(&$crate::prelude::format!("{:?}", $arg)).unwrap()
    };
    ($fmt:literal $(, $args:expr)+) => {
        $crate::ext::debug(&$crate::prelude::format!($fmt $(, $args)+)).unwrap()
    };
}

#[cfg(not(feature = "debug"))]
#[cfg(not(debug_assertions))]
#[macro_export]
macro_rules! debug {
    ($arg:expr) => { let _ = $arg; };
    ($fmt:literal $(, $args:expr)+) => {
        let _ = $fmt;
        $(let _ = $args;) +
    };
}
