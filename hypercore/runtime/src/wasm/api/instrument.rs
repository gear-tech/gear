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

use alloc::vec::Vec;
use gear_core::code::{Code, CodeAndId, InstrumentedCode, InstrumentedCodeAndId};
use gear_wasm_instrument::gas_metering::Schedule;

// TODO: return Result here.
pub fn instrument(code: Vec<u8>) -> Option<InstrumentedCode> {
    log::info!("You're calling 'instrument(..)'");

    let schedule = Schedule::default();

    // TODO: consider runtime version here.
    match Code::try_new(
        code,
        schedule.instruction_weights.version,
        |module| schedule.rules(module),
        schedule.limits.stack_height,
        schedule.limits.data_segments_amount.into(),
    ) {
        Ok(instrumented) => {
            if instrumented.code().len() > schedule.limits.code_len as usize {
                log::debug!("Code is too big!");
                return None;
            }

            let code_and_id = CodeAndId::new(instrumented);

            // TODO: fix this strange casts.
            let instrumented = InstrumentedCodeAndId::from(code_and_id).into_parts().0;

            Some(instrumented)
        }
        Err(e) => {
            log::debug!("Bad instrumentation: {e:?}!");
            None
        }
    }
}
