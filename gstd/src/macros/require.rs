// This file is part of Gear.

// Copyright (C) 2021 Gear Technologies Inc.
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

//! Gear `require!` macro.
//! Check the available balances and panics if they less then passed argument

#[macro_export]
macro_rules! require {
    (GAS $arg:expr) => {
        if $crate::exec::gas_available() < $arg as u64 {
            panic!("Required gas amount is less than available");
        }
    };
    (VALUE $arg:expr) => {
        if $crate::exec::value_available() < $arg as u128 {
            panic!("Required value amount is less than available");
        }
    };
}
