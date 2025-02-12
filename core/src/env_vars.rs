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

//! Execution settings

use core::slice;
pub use gsys::EnvVars as EnvVarsV1;

/// All supported versions of execution settings
pub enum EnvVars {
    /// Values of execution settings V1
    // When a new version is introduced in gsys, the previous version should be
    // copied here as EnvVarsV1 whereas the most recent version should be bound
    // to the variant corresponding to it
    V1(EnvVarsV1),
}

impl EnvVars {
    /// Returns byte representation of execution settings
    pub fn to_bytes(&self) -> &[u8] {
        match self {
            EnvVars::V1(v1) => {
                let ptr = v1 as *const EnvVarsV1 as *const u8;
                unsafe { slice::from_raw_parts(ptr, size_of::<EnvVarsV1>()) }
            }
        }
    }
}
