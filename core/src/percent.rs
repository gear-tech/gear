// This file is part of Gear.

// Copyright (C) 2021-2025 Gear Technologies Inc.
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

//! Basic struct for working with integer percentages.

use core::cmp::Ord;
use num_traits::{Num, cast::NumCast};
use parity_scale_codec::{Decode, Encode};
use scale_info::TypeInfo;

/// Basic struct for working with integer percentages.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Encode, Decode, TypeInfo)]
pub struct Percent(u32);

impl Percent {
    /// Creates a new `Percent` from a `u32` value. The value can be
    /// greater than 100.
    pub fn new(value: u32) -> Self {
        Self(value)
    }

    /// Returns the inner `u32` value.
    pub fn value(self) -> u32 {
        self.0
    }

    /// Applies the percentage to the given value.
    pub fn apply_to<T: Num + Ord + Copy + NumCast>(&self, value: T) -> T {
        (value * NumCast::from(self.0).unwrap()) / NumCast::from(100).unwrap()
    }
}

impl From<u32> for Percent {
    fn from(value: u32) -> Self {
        Self::new(value)
    }
}

impl From<Percent> for gsys::Percent {
    fn from(value: Percent) -> Self {
        gsys::Percent::new(value.value())
    }
}
