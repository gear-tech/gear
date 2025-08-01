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

//! Declares modules, attributes, public re-exports.
//! Gear libs are `#![no_std]`, which makes them lightweight.

//! This module is for configuring `gstd` inside gear programs.
use crate::{
    BlockCount,
    errors::{Error, Result, UsageError},
};

/// Constant declaring default `Config::system_reserve()` in case of not
/// "ethexe" feature.
pub const SYSTEM_RESERVE: u64 = 10_000_000_000;

/// Wait types.
#[derive(Clone, Copy, Default)]
pub(crate) enum WaitType {
    WaitFor,
    #[default]
    WaitUpTo,
}

/// The set of broadly used internal parameters.
///
/// These parameters have various predefined values that the program developer
/// can override.
pub struct Config {
    /// Default wait duration for `wait_up_to` messages expressed in block
    /// count.
    ///
    /// Initial value: **100 blocks**
    pub wait_up_to: BlockCount,

    /// Default wait duration for `wait_for` messages expressed in block count.
    ///
    /// Initial value: **100 blocks**
    pub wait_for: BlockCount,

    /// Default amount of blocks a mutex lock can be owned for by a message.
    ///
    /// Initial value: **100 blocks**
    pub mx_lock_duration: BlockCount,

    /// Default gas amount reserved for system purposes.
    ///
    /// Initial value: **1_000_000_000**
    #[cfg(not(feature = "ethexe"))]
    pub system_reserve: u64,

    pub(crate) wait_type: WaitType,
}

impl Config {
    const fn default() -> Self {
        Self {
            wait_up_to: 100,
            wait_for: 100,
            mx_lock_duration: 100,
            #[cfg(not(feature = "ethexe"))]
            system_reserve: SYSTEM_RESERVE,
            wait_type: WaitType::WaitUpTo,
        }
    }

    pub(crate) fn wait_type() -> WaitType {
        unsafe { CONFIG.wait_type }
    }

    /// Get the `wait_for` duration (in blocks).
    pub fn wait_for() -> BlockCount {
        unsafe { CONFIG.wait_for }
    }

    /// Get the `wait_up_to` duration (in blocks).
    pub fn wait_up_to() -> BlockCount {
        unsafe { CONFIG.wait_up_to }
    }

    /// Get the `mx_lock_duration` duration (in blocks).
    pub fn mx_lock_duration() -> BlockCount {
        unsafe { CONFIG.mx_lock_duration }
    }

    /// Get the `system_reserve` gas amount.
    #[cfg(not(feature = "ethexe"))]
    pub fn system_reserve() -> u64 {
        unsafe { CONFIG.system_reserve }
    }

    /// Set `wait_for` duration (in blocks).
    pub fn set_wait_for(duration: BlockCount) -> Result<()> {
        if duration == 0 {
            return Err(Error::Gstd(UsageError::EmptyWaitDuration));
        }

        unsafe { CONFIG.wait_for = duration };
        Ok(())
    }

    /// Set `wait_for` as the default wait type with duration.
    ///
    /// Calling this function forces all async functions that wait for some
    /// condition to wait exactly for `duration` blocks.
    pub fn set_default_wait_for(duration: BlockCount) -> Result<()> {
        Self::set_wait_for(duration)?;
        unsafe { CONFIG.wait_type = WaitType::WaitFor };

        Ok(())
    }

    /// Set the `wait_up_to` duration (in blocks).
    pub fn set_wait_up_to(duration: BlockCount) -> Result<()> {
        if duration == 0 {
            return Err(Error::Gstd(UsageError::EmptyWaitDuration));
        }

        unsafe { CONFIG.wait_up_to = duration };
        Ok(())
    }

    /// Set `wait_up_to` as the default wait type with duration.
    ///
    /// Calling this function forces all async functions that wait for some
    /// condition to wait not more than `duration` blocks.
    pub fn set_default_wait_up_to(duration: BlockCount) -> Result<()> {
        Self::set_wait_up_to(duration)?;
        unsafe { CONFIG.wait_type = WaitType::WaitUpTo };

        Ok(())
    }

    /// Set `mx_lock_duration_max` duration (in blocks).
    pub fn set_mx_lock_duration(duration: BlockCount) -> Result<()> {
        if duration == 0 {
            return Err(Error::Gstd(UsageError::ZeroMxLockDuration));
        }

        unsafe { CONFIG.mx_lock_duration = duration };
        Ok(())
    }

    /// Set `system_reserve` gas amount.
    #[cfg(not(feature = "ethexe"))]
    pub fn set_system_reserve(amount: u64) -> Result<()> {
        if amount == 0 {
            return Err(Error::Gstd(UsageError::ZeroSystemReservationAmount));
        }

        unsafe { CONFIG.system_reserve = amount };
        Ok(())
    }
}

// Private `gstd` configuration, only could be modified
// with the public interfaces of `Config`.
static mut CONFIG: Config = Config::default();
