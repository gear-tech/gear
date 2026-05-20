// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

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
