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

//! Declares modules, attributes, public re-exports.
//! Gear libs are `#![no_std]`, which makes them lightweight.

//! This module is for configuring `gstd` inside gear programs.

/// `gstd` configuration
pub struct Config {
    /// Default wait duration for `wait_up_to` messages.
    pub wait_up_to: u32,

    /// Default wait duration for `wait_for` messages.
    pub wait_for: u32,
}

impl Config {
    const fn default() -> Self {
        Self {
            wait_up_to: 100,
            wait_for: 100,
        }
    }

    /// Get `wait_for` duration
    pub fn wait_for() -> u32 {
        unsafe { CONFIG.wait_for }
    }

    /// Get `wait_up_to` duration
    pub fn wait_up_to() -> u32 {
        unsafe { CONFIG.wait_up_to }
    }

    /// Set `wait_for` duration
    pub fn set_wait_for(duration: u32) {
        unsafe { CONFIG.wait_for = duration };
    }

    /// Set `wait_up_to` duration
    pub fn set_wait_up_to(duration: u32) {
        unsafe { CONFIG.wait_up_to = duration };
    }
}

// Private `gstd` configuration, only could be modified
// with the public interfaces of `Config`.
static mut CONFIG: Config = Config::default();
