// This file is part of Gear.

// Copyright (C) 2025 Gear Technologies Inc.
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

#![no_std]

extern crate alloc;

#[cfg(any(feature = "bls12-381", feature = "bls12-381-std"))]
pub mod bls12_381;

use parity_scale_codec::{Decode, Encode};
use gear_core::{gas::{ChargeResult, GasAllowanceCounter, GasAmount, GasCounter}, str::LimitedStr};

/// A builtin actor execution context. Primarily used to track gas usage.
#[derive(Debug)]
pub struct BuiltinContext {
    pub(crate) gas_counter: GasCounter,
    pub(crate) gas_allowance_counter: GasAllowanceCounter,
}

impl BuiltinContext {
    pub fn new(counter_initial: u64, allowance_initial: u64) -> Self {
        Self {
            gas_counter: GasCounter::new(counter_initial),
            gas_allowance_counter: GasAllowanceCounter::new(allowance_initial),
        }
    }

    // Tries to charge the gas amount from the gas counters.
    pub fn try_charge_gas(&mut self, amount: u64) -> Result<(), BuiltinActorError> {
        if self.gas_counter.charge_if_enough(amount) == ChargeResult::NotEnough {
            return Err(BuiltinActorError::InsufficientGas);
        }

        if self.gas_allowance_counter.charge_if_enough(amount) == ChargeResult::NotEnough {
            return Err(BuiltinActorError::GasAllowanceExceeded);
        }

        Ok(())
    }

    // Checks if an amount of gas can be charged without actually modifying the inner counters.
    pub fn can_charge_gas(&self, amount: u64) -> Result<(), BuiltinActorError> {
        if self.gas_counter.left() < amount {
            return Err(BuiltinActorError::InsufficientGas);
        }

        if self.gas_allowance_counter.left() < amount {
            return Err(BuiltinActorError::GasAllowanceExceeded);
        }

        Ok(())
    }

    pub fn to_gas_amount(&self) -> GasAmount {
        self.gas_counter.to_amount()
    }
}

/// Built-in actor error type
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, derive_more::Display)]
pub enum BuiltinActorError {
    /// Occurs if the underlying call has the weight greater than the `gas_limit`.
    #[display("Not enough gas supplied")]
    InsufficientGas,
    /// Occurs if the dispatch's value is less than the minimum required value.
    #[display("Not enough value supplied")]
    InsufficientValue,
    /// Occurs if the dispatch's message can't be decoded into a known type.
    #[display("Failure to decode message")]
    DecodingError,
    /// Actor's inner error encoded as a String.
    #[display("Builtin execution resulted in error: {_0}")]
    Custom(LimitedStr<'static>),
    /// Occurs if a builtin actor execution does not fit in the current block.
    #[display("Block gas allowance exceeded")]
    GasAllowanceExceeded,
}
