// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Entities describing configuration for precise syscalls.

use std::ops::RangeInclusive;

/// Represents the configuration for building some parts of precise syscalls.
/// Can be used to write unit tests so you don't have to rely on randomness.
#[derive(Debug, Clone)]
pub struct PreciseSyscallsConfig {
    range_of_send_push_calls: RangeInclusive<usize>,
    range_of_send_input_calls: RangeInclusive<usize>,
}

impl PreciseSyscallsConfig {
    /// Creates a new configuration for precise syscalls, filled with the given values.
    pub fn new(
        range_of_send_push_calls: RangeInclusive<usize>,
        range_of_send_input_calls: RangeInclusive<usize>,
    ) -> Self {
        Self {
            range_of_send_push_calls,
            range_of_send_input_calls,
        }
    }

    /// Get the range of `send_push*` syscalls.
    pub fn range_of_send_push_calls(&self) -> RangeInclusive<usize> {
        self.range_of_send_push_calls.clone()
    }

    /// Get the range of `send_input*` syscalls.
    pub fn range_of_send_input_calls(&self) -> RangeInclusive<usize> {
        self.range_of_send_input_calls.clone()
    }
}

impl Default for PreciseSyscallsConfig {
    fn default() -> Self {
        Self::new(0..=3, 1..=1)
    }
}
