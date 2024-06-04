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
use alloc::boxed::Box;
use gear_core::code::Code;
use gear_wasm_instrument::gas_metering::Schedule;
use parity_scale_codec::Encode as _;

pub fn instrument(code_len: usize) -> i64 {
    log::info!("You're calling 'instrument(code_len={code_len})'");

    let code = code_ri::load(code_len);

    let schedule = Schedule::default();

    // TODO: consider runtime version here.
    match Code::try_new(
        code,
        schedule.instruction_weights.version,
        |module| schedule.rules(module),
        schedule.limits.stack_height,
        schedule.limits.data_segments_amount.into(),
    ) {
        Ok(code) => {
            let instrumented = code.into_parts().0;

            if instrumented.code().len() > schedule.limits.code_len as usize {
                log::debug!("Code is too big!");
                return 0;
            }

            let encoded = instrumented.encode();

            let len = encoded.len() as i32;
            let ptr = Box::leak(Box::new(encoded)).as_ptr() as i32;

            unsafe { core::mem::transmute([ptr, len]) }
        }
        Err(e) => {
            log::debug!("Bad instrumentation: {e:?}!");
            0
        }
    }
}
