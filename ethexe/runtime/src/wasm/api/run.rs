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

use crate::wasm::storage::NativeRuntimeInterface;
use ethexe_runtime_common::{ProgramJournals, RuntimeRunContext, process_queue};

pub fn run(ctx: RuntimeRunContext) -> (ProgramJournals, u64) {
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
