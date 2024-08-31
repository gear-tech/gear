// This file is part of Gear.

// Copyright (C) 2024 Gear Technologies Inc.
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

//! GasAllowance implementation for gtest.
//!
//! Handles burning of gas for specific IDs as well.

use std::collections::BTreeMap;

use gear_common::{MessageId, Origin};

use crate::Gas;

#[derive(Default, Debug)]
pub struct GasAllowance {
    gas_burned: BTreeMap<MessageId, Gas>,
    allowance: Gas,
}

impl GasAllowance {
    pub fn put(&mut self, gas_allowance: Gas) {
        self.allowance = gas_allowance;
    }

    pub fn decrease(&mut self, gas: Gas) {
        self.allowance = self.allowance.saturating_sub(gas);
    }

    pub fn get(&self) -> Gas {
        self.allowance
    }

    pub fn burn(&mut self, id: impl Origin, gas: Gas) {
        self.gas_burned
            .entry(id.cast())
            .and_modify(|v| *v = v.saturating_add(gas))
            .or_insert(gas);
        self.decrease(gas);
    }

    pub fn gas_burned(&self) -> &BTreeMap<MessageId, Gas> {
        &self.gas_burned
    }

    pub fn take_gas_burned(&mut self) -> BTreeMap<MessageId, Gas> {
        std::mem::take(&mut self.gas_burned)
    }
}
