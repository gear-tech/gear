// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use crate::wasm::storage::NativeRuntimeInterface;
use ethexe_runtime_common::{ProcessQueueContext, ProgramJournals, process_queue};

/// Processes the program message queue for one block, returning execution journals and total gas spent.
///
/// Delegates to [`process_queue`] with a [`NativeRuntimeInterface`] instance, then
/// logs each resulting journal note at debug level.
pub fn run(ctx: ProcessQueueContext) -> (ProgramJournals, u64) {
    log::debug!("You're calling 'run(..)'");

    let ri = NativeRuntimeInterface;

    let (journals, gas_spent) = process_queue(ctx, &ri);

    for (journal, message_type, call_reply) in &journals {
        for note in journal {
            log::debug!("{note:?}");
        }
        log::debug!("Message type: {message_type:?}, call_reply {call_reply:?}");
    }

    (journals, gas_spent)
}
