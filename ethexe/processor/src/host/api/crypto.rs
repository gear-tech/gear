// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Native implementations of `gr_crypto` operations.
//!
//! The runtime forwards the raw op id and input buffer here; results are
//! written back into runtime memory. All implementations must be strictly
//! deterministic — they are part of state computation replicated across
//! validators.

use crate::host::{StoreData, context};
use gsys::CryptoOp;
use wasmtime::{Caller, Linker};

pub fn link(linker: &mut Linker<StoreData>) -> Result<(), wasmtime::Error> {
    linker.func_wrap("env", "ext_crypto_version_1", crypto)?;

    Ok(())
}

/// Returns 0 on success (result written to `output_ptr`), 1 on malformed
/// input or unknown op.
fn crypto(mut caller: Caller<'_, StoreData>, op: u32, input_ptr_len: i64, output_ptr: u32) -> i32 {
    let Some(op) = CryptoOp::from_u32(op) else {
        log::trace!("ext_crypto: unknown op {op}");
        return 1;
    };

    let mut memory = context::memory(&mut caller);
    let input = memory.slice_by_val(input_ptr_len).to_vec();

    let Some(result) = ops::execute(op, &input) else {
        log::trace!("ext_crypto: malformed input for {op:?}");
        return 1;
    };
    debug_assert_eq!(result.len(), op.output_len() as usize);

    let Some(output) = memory.slice_mut(output_ptr, op.output_len()) else {
        log::trace!("ext_crypto: output buffer out of bounds for {op:?}");
        return 1;
    };
    output.copy_from_slice(&result);

    0
}

pub(crate) mod ops {
    pub use ethexe_runtime_common::crypto_ops::*;
}
