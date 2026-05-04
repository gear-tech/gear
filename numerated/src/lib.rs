// This file is part of Gear.

// Copyright (C) 2023-2025 Gear Technologies Inc.
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

//! Crate for working with [`Numerated`] types and their sets: [`Interval`](interval::Interval)
//! and [`IntervalsTree`](tree::IntervalsTree).
//!
//! ### Note
//! In case [`Numerated`] is implemented incorrectly for some type `T`, then this can cause
//! incorrect behavior of [`IntervalsTree`](tree::IntervalsTree) and
//! [`Interval`](interval::Interval) for `T`.

#![no_std]
#![deny(missing_docs)]

extern crate alloc;

pub mod interval;
pub mod iterators;
mod numerated;
pub mod tree;

pub use num_traits;
pub use numerated::*;

#[cfg(any(feature = "mock", test))]
pub mod mock;

#[cfg(test)]
mod tests;
