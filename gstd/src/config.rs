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
use crate::errors::{ContractError, Result};

/// Wait types.
#[derive(Clone, Copy, Default)]
pub(crate) enum WaitType {
    WaitFor,
    #[default]
    WaitUpTo,
}

/// `gstd` configuration
pub struct Config {
    /// Default wait duration for `wait_up_to` messages.
    pub wait_up_to: u32,

    /// Default wait duration for `wait_for` messages.
    pub wait_for: u32,

    /// Default system reservation gas amount.
    pub system_reserve: u64,

    /// Default wait type for `wait` messages.
    pub(crate) wait_type: WaitType,
}

impl Config {
    const fn default() -> Self {
        Self {
            wait_up_to: 100,
            wait_for: 100,
            system_reserve: 1_000_000_000,
            wait_type: WaitType::WaitUpTo,
        }
    }

    // Get default wait type.
    pub(crate) fn wait_type() -> WaitType {
        unsafe { CONFIG.wait_type }
    }

    /// Get `wait_for` duration
    pub fn wait_for() -> u32 {
        unsafe { CONFIG.wait_for }
    }

    /// Get `wait_up_to` duration
    pub fn wait_up_to() -> u32 {
        unsafe { CONFIG.wait_up_to }
    }

    pub fn system_reserve() -> u64 {
        unsafe { CONFIG.system_reserve }
    }

    /// Set `wait_for` duration
    pub fn set_wait_for(duration: u32) -> Result<()> {
        if duration == 0 {
            return Err(ContractError::EmptyWaitDuration);
        }

        unsafe { CONFIG.wait_for = duration };
        Ok(())
    }

    /// Set `wait_for` as default wait type with duration.
    pub fn set_default_wait_for(duration: u32) -> Result<()> {
        Self::set_wait_for(duration)?;
        unsafe { CONFIG.wait_type = WaitType::WaitFor };

        Ok(())
    }

    /// Set `wait_up_to` duration
    pub fn set_wait_up_to(duration: u32) -> Result<()> {
        if duration == 0 {
            return Err(ContractError::EmptyWaitDuration);
        }

        unsafe { CONFIG.wait_up_to = duration };
        Ok(())
    }

    /// Set `wait_up_to` as default wait type with duration.
    pub fn set_default_wait_up_to(duration: u32) -> Result<()> {
        Self::set_wait_up_to(duration)?;
        unsafe { CONFIG.wait_type = WaitType::WaitUpTo };

        Ok(())
    }

    pub fn set_system_reserve(amount: u64) -> Result<()> {
        if amount == 0 {
            return Err(ContractError::ZeroSystemReservationAmount);
        }

        unsafe { CONFIG.system_reserve = amount };
        Ok(())
    }
}

// Private `gstd` configuration, only could be modified
// with the public interfaces of `Config`.
static mut CONFIG: Config = Config::default();
