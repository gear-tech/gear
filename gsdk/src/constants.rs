// This file is part of Gear.
//
// Copyright (C) 2021-2025 Gear Technologies Inc.
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

//! Runtime constants query methods

use crate::{
    Api,
    gear::{self, runtime_types::gear_common::GasMultiplier},
    result::Result,
};
use sp_runtime::Percent;
use subxt::{constants, utils::AccountId32};

impl Api {
    /// Query constant
    pub fn constant<Addr: constants::Address>(&self, addr: &Addr) -> Result<Addr::Target> {
        self.constants().at(addr).map_err(Into::into)
    }
}

// pallet-babe
impl Api {
    /// Get expected block time.
    pub fn expected_block_time(&self) -> Result<u64> {
        self.constant(&gear::constants().babe().expected_block_time())
    }
}

// pallet-gear-bank
impl Api {
    /// Get gas multiplier.
    pub fn gas_multiplier(&self) -> Result<GasMultiplier<u128, u64>> {
        self.constant(&gear::constants().gear_bank().gas_multiplier())
    }

    /// Get treasury address set.
    pub fn treasury_address(&self) -> Result<AccountId32> {
        self.constant(&gear::constants().gear_bank().treasury_address())
    }

    /// Get treasury gas payouts fee percent.
    pub fn treasury_gas_fee_share(&self) -> Result<Percent> {
        self.constant(&gear::constants().gear_bank().treasury_gas_fee_share())
            .map(|p| p.0)
    }

    /// Get treasury tx fee percent.
    pub fn treasury_tx_fee_share(&self) -> Result<Percent> {
        self.constant(&gear::constants().gear_bank().treasury_tx_fee_share())
            .map(|p| p.0)
    }
}

// pallet-gear-gas
impl Api {
    /// Get gas limit.
    pub fn gas_limit(&self) -> Result<u64> {
        self.constant(&gear::constants().gear_gas().block_gas_limit())
    }
}
