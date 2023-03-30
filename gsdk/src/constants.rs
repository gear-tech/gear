// This file is part of Gear.
//
// Copyright (C) 2021-2022 Gear Technologies Inc.
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

//! gear api constants methods
use crate::{result::Result, Api};
use parity_scale_codec::Decode;

impl Api {
    /// pallet gas constants
    ///
    /// Get gas limit.
    pub fn gas_limit(&self) -> Result<u64> {
        let addr = subxt::dynamic::constant("GearGas", "BlockGasLimit");
        Ok(u64::decode(&mut self.constants().at(&addr)?.encoded())?)
    }

    /// pallet babe constant
    ///
    /// Get expected block time.
    pub fn expected_block_time(&self) -> Result<u64> {
        let addr = subxt::dynamic::constant("Babe", "ExpectedBlockTime");
        Ok(u64::decode(&mut self.constants().at(&addr)?.encoded())?)
    }
}
