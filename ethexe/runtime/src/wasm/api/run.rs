// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use crate::wasm::storage::NativeRuntimeInterface;
use ethexe_runtime_common::{ProcessQueueContext, ProgramJournals, process_queue};

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
