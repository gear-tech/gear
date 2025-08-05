// This file is part of Gear.
//
// Copyright (C) 2024-2025 Gear Technologies Inc.
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
use gear_core::{
    code::{Code, CodeError, CodeMetadata, InstrumentedCode},
    gas_metering::Schedule,
};

// TODO: impl Codec for CodeError, so could be thrown to host via memory.
pub fn instrument_code(original_code: Vec<u8>) -> Option<(InstrumentedCode, CodeMetadata)> {
    log::debug!("Runtime::instrument_code(..)");

    let schedule = Schedule::default();

    if original_code.len() > schedule.limits.code_len as usize {
        log::debug!("Original code exceeds size limit!");
        return None;
    }

    let code = Code::try_new(
        original_code,
        // TODO: should we update it on each upgrade (?);
        ethexe_runtime_common::VERSION,
        |module| schedule.rules(module),
        schedule.limits.stack_height,
        schedule.limits.data_segments_amount.into(),
    )
    .map_err(|e: CodeError| {
        log::debug!("Failed to validate or instrument code: {e:?}");
        e
    })
    .ok()?;

    let (_, instrumented, metadata) = code.into_parts();

    if instrumented.bytes().len() > schedule.limits.code_len as usize {
        log::debug!("Instrumented code exceeds size limit!");
        return None;
    }

    Some((instrumented, metadata))
}
