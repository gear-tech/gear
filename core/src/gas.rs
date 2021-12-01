// This file is part of Gear.

// Copyright (C) 2021 Gear Technologies Inc.
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

//! Gas module.

use alloc::vec::Vec;

/// The result of charging gas.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ChargeResult {
    /// There was enough gas and it has been charged.
    Enough,
    /// There was not enough gas and it hasn't been charged.
    NotEnough,
}

/// Instrumentation error.
#[derive(Debug)]
pub enum InstrumentError {
    /// Error occurred during decoding original program code.
    ///
    /// The provided code was a malformed Wasm bytecode or contained unsupported features
    /// (atomics, simd instructions, etc.).
    Decode,
    /// Error occurred during injecting gas metering instructions.
    ///
    /// This might be due to program contained unsupported/non-deterministic instructions
    /// (floats, manual memory grow, etc.).
    GasInjection,
    /// Error occurred during encoding instrumented program.
    ///
    /// The only possible reason for that might be OOM.
    Encode,
}

/// Gas counter with some predefined maximum gas.
#[derive(Debug)]
pub struct GasCounter {
    left: u64,
    burned: u64,
}

impl GasCounter {
    /// New limited gas counter with initial gas to spend.
    pub fn new(initial_amount: u64) -> Self {
        Self {
            left: initial_amount,
            burned: 0,
        }
    }

    /// Charge `amount` of gas.
    pub fn charge(&mut self, amount: u64) -> ChargeResult {
        if self.left < amount {
            return ChargeResult::NotEnough;
        }

        self.left -= amount;
        self.burned += amount;

        ChargeResult::Enough
    }

    /// Reduce gas by `amount`.
    ///
    /// Called when message is sent to another program, so the gas `amount` is sent to
    /// receiving program.
    pub fn reduce(&mut self, amount: u64) -> ChargeResult {
        if self.left < amount {
            return ChargeResult::NotEnough;
        }

        self.left -= amount;

        ChargeResult::Enough
    }

    /// Refund `amount` of gas.
    pub fn refund(&mut self, amount: u64) -> ChargeResult {
        if amount > u64::MAX - self.left || amount > self.burned {
            return ChargeResult::NotEnough;
        }

        self.left += amount;
        self.burned -= amount;

        ChargeResult::Enough
    }

    /// Report how much gas is left.
    pub fn left(&self) -> u64 {
        self.left
    }

    /// Report how much gas is burned.
    pub fn burned(&self) -> u64 {
        self.burned
    }
}

/// Instrument code with gas-counting instructions.
pub fn instrument(code: &[u8]) -> Result<Vec<u8>, InstrumentError> {
    use pwasm_utils::rules::{InstructionType, Metering};

    let module = parity_wasm::elements::Module::from_bytes(code).map_err(|e| {
        log::error!("Error decoding module: {}", e);
        InstrumentError::Decode
    })?;

    let instrumented_module = pwasm_utils::inject_gas_counter(
        module,
        &pwasm_utils::rules::Set::new(
            // TODO: put into config/processing
            1000,
            // Memory.grow is forbidden
            [(InstructionType::GrowMemory, Metering::Forbidden)]
                .iter()
                .cloned()
                .collect(),
        )
        .with_forbidden_floats(),
        "env",
    )
    .map_err(|_module| {
        log::error!("Error injecting gas counter");
        InstrumentError::GasInjection
    })?;

    parity_wasm::elements::serialize(instrumented_module).map_err(|e| {
        log::error!("Error encoding module: {}", e);
        InstrumentError::Encode
    })
}

#[cfg(test)]
mod tests {
    use super::{ChargeResult, GasCounter};

    #[test]
    /// Test that `GasCounter` object returns `Enough` and decreases the remaining count
    /// on calling `charge(...)` when the remaining gas exceeds the required value,
    /// otherwise returns NotEnough
    fn limited_gas_counter_charging() {
        let mut counter = GasCounter::new(200);

        let result = counter.charge(100);

        assert_eq!(result, ChargeResult::Enough);
        assert_eq!(counter.left(), 100);

        let result = counter.charge(101);

        assert_eq!(result, ChargeResult::NotEnough);
        assert_eq!(counter.left(), 100);
    }
}
