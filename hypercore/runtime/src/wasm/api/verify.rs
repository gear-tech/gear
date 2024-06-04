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

use crate::wasm::interface::code_ri;
use gear_core::code::{Code, CodeAndId};
use gear_wasm_instrument::gas_metering::{CustomConstantCostRules, Schedule};

pub fn verify(code_len: usize) -> bool {
    log::info!("You're calling 'verify(code_len={code_len})'");

    let schedule = Schedule::default();

    if code_len > schedule.limits.code_len as usize {
        log::debug!("Code is too big!");
        return false;
    }

    let code = code_ri::load(code_len);

    // TODO: only check code here.
    match Code::try_new(code, 42, |_| CustomConstantCostRules::default(), None, None) {
        Ok(code) => {
            let code_id = CodeAndId::new(code).code_id();
            log::debug!("Nice code! Code id is {code_id}");
            true
        }
        Err(e) => {
            log::debug!("Bad code: {e:?}!");
            false
        }
    }
}
