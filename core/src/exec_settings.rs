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

//! Execution settings

use core::{mem, slice};

/// All supported versions of execution settings
pub enum ExecSettings {
    /// Values of execution settings V1
    V1(ExecSettingsV1),
}

impl ExecSettings {
    /// Returns byte representation of execution settings
    pub fn to_bytes(&self) -> &[u8] {
        match self {
            ExecSettings::V1(v1) => {
                let ptr = v1 as *const ExecSettingsV1 as *const u8;
                unsafe { slice::from_raw_parts(ptr, mem::size_of::<ExecSettingsV1>()) }
            }
        }
    }
}

/// Values of execution settings V1
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct ExecSettingsV1 {
    /// Performance multiplier percentage
    pub performance_multiplier_percent: u32,
    /// Existential deposit
    pub existential_deposit: u128,
    /// Mailbox threshold
    pub mailbox_threshold: u64,
    /// Multiplier for converting gas into value
    pub gas_to_value_multiplier: u128,
}
