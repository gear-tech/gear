// This file is part of Gear.

// Copyright (C) 2022 Gear Technologies Inc.
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

//! Module for callback primitives.
//!
//! Callbacks represent some additional logic which
//! should be done over the argument on some conditions.

/// Represents callback action for cases `(&T) -> R`,
/// where `R` is `()` by default.
pub trait Callback<T, R = ()> {
    /// Triggers the callback's logic.
    fn call(arg: &T) -> R;
}

// Blank `Callback<T, ()>` implementation
// for skipping callback type in parent traits.
impl<T> Callback<T> for () {
    fn call(_: &T) {}
}

/// Represents callback action for cases
/// without input and output.
pub trait EmptyCallback {
    /// Triggers the callback's logic.
    fn call();
}

// Blank `EmptyCallback` implementation
// for skipping callback type in parent traits.
impl EmptyCallback for () {
    fn call() {}
}

/// Represents callback action for cases `(&T) -> Result<R, E>`,
/// where `R` is `()` by default and `E` is associated type `Error`.
pub trait FallibleCallback<T, R = ()> {
    /// Returning error in callback's `Err` case.
    type Error;

    /// Triggers the callback's logic.
    fn call(arg: &T) -> Result<R, Self::Error>;
}

/// Represents callback action for cases
/// without input for getting some data.
pub trait GetCallback<T> {
    /// Returns value by callback's logic.
    fn call() -> T;
}

// Blank `GetCallback` implementation
// for returning default values.
impl<T: Default> GetCallback<T> for () {
    fn call() -> T {
        Default::default()
    }
}

/// Represents transposing callback
/// for mutating values.
pub trait TransposeCallback<T, R> {
    /// Returns value by callback's logic.
    fn call(arg: T) -> R;
}

// Blank `TransposeCallback` implementation
// for returning value itself.
impl<T> TransposeCallback<T, T> for () {
    fn call(arg: T) -> T {
        arg
    }
}
