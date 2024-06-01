// This file is part of Gear.
//
// Copyright (C) 2024 Gear Technologies Inc.
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

use crate::interface::{code_ri, program_ri};
use gear_core::code::Code;
use gear_wasm_instrument::gas_metering::CustomConstantCostRules;

pub fn verify() {
    let code_id = code_ri::id();

    log::info!("You're calling 'verify({code_id:.4})'");

    let code = code_ri::read(code_id);

    assert!(
        Code::try_new(code, 42, |_| CustomConstantCostRules::default(), None, None,).is_ok(),
        "Submitted code is invalid"
    );
}
