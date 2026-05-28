// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use alloc::vec::Vec;
use gear_core::{
    code::{Code, CodeError, CodeMetadata, InstrumentedCode, SyscallKind},
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
        schedule.limits.type_section_len.into(),
        schedule.limits.parameters.into(),
        SyscallKind::Eth,
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
