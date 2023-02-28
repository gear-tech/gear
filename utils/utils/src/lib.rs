// This file is part of Gear.

// Copyright (C) 2021-2023 Gear Technologies Inc.
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

//! Useful utilities needed for testing and other stuff.

pub use nonempty::NonEmpty;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Trait describes a collection which can get a value by it's index.
/// The index can be in any range, even [length(implementor), ..).
///
/// The key feature of the trait is that the implementor should guarantee
/// that with under provided index there's always some value. The best way
/// to do that is to implement a trait for a guaranteed not empty type.
pub trait RingGet<V> {
    /// Returns with a guarantee a value under `index`.
    fn ring_get(&self, index: usize) -> &V;
}

impl<V> RingGet<V> for NonEmpty<V> {
    fn ring_get(&self, index: usize) -> &V {
        // Guaranteed to have value, because index is in the range of [0; self.len()).
        self.get(index % self.len()).expect("guaranteed to be some")
    }
}

/// Returns time elapsed since [`UNIX_EPOCH`] in milliseconds.
pub fn now_millis() -> u64 {
    now_duration().as_millis() as u64
}

/// Returns time elapsed since [`UNIX_EPOCH`] in microseconds.
pub fn now_micros() -> u128 {
    now_duration().as_micros()
}

/// Returns [`Duration`] from [`UNIX_EPOCH`] until now.
pub fn now_duration() -> Duration {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Internal error: current time before UNIX Epoch")
}

/// Initialize a simple logger from env.
///
/// Does show:
/// - level
/// - timestamp
/// - module
///
/// Does not show
/// - module path
pub fn init_default_logger() {
    let _ = env_logger::Builder::from_default_env()
        .format_module_path(false)
        .format_level(true)
        .try_init();
}
