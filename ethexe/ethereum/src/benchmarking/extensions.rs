// This file is part of Gear.
//
// Copyright (C) 2024-2026 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use revm::primitives::Bytes;

#[derive(Debug)]
pub struct CalldataGas {
    pub zero_bytes: usize,
    pub non_zero_bytes: usize,
}

impl CalldataGas {
    pub fn total_gas(&self) -> u64 {
        (self.zero_bytes * 4 + self.non_zero_bytes * 16) as u64
    }
}

pub trait CalldataGasExt {
    fn calldata_gas(&self) -> CalldataGas;
}

impl CalldataGasExt for Bytes {
    fn calldata_gas(&self) -> CalldataGas {
        let zero_bytes = self.iter().filter(|&&b| b == 0).count();
        let non_zero_bytes = self.len() - zero_bytes;

        CalldataGas {
            zero_bytes,
            non_zero_bytes,
        }
    }
}
