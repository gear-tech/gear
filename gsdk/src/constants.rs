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

use crate::{Api, metadata::runtime_types::gear_common::GasMultiplier, result::Result};
use parity_scale_codec::Decode;
use sp_runtime::Percent;
use subxt::utils::AccountId32;

impl Api {
    /// Query constant
    fn query_constant<D: Decode>(&self, pallet: &'static str, constant: &'static str) -> Result<D> {
        let addr = subxt::dynamic::constant(pallet, constant);
        D::decode(&mut self.constants().at(&addr)?.encoded()).map_err(Into::into)
    }
}

// pallet-babe
impl Api {
    /// Get expected block time.
    pub fn expected_block_time(&self) -> Result<u64> {
        self.query_constant("Babe", "ExpectedBlockTime")
    }
}

// pallet-gear-bank
impl Api {
    /// Get gas multiplier.
    pub fn gas_multiplier(&self) -> Result<GasMultiplier<u128, u64>> {
        self.query_constant("GearBank", "GasMultiplier")
    }

    /// Get treasury address set.
    pub fn treasury_address(&self) -> Result<AccountId32> {
        self.query_constant("GearBank", "TreasuryAddress")
    }

    /// Get treasury gas payouts fee percent.
    pub fn treasury_gas_fee_share(&self) -> Result<Percent> {
        self.query_constant("GearBank", "TreasuryGasFeeShare")
    }

    /// Get treasury tx fee percent.
    pub fn treasury_tx_fee_share(&self) -> Result<Percent> {
        self.query_constant("GearBank", "TreasuryTxFeeShare")
    }
}

// pallet-gear-gas
impl Api {
    /// Get gas limit.
    pub fn gas_limit(&self) -> Result<u64> {
        self.query_constant("GearGas", "BlockGasLimit")
    }
}
