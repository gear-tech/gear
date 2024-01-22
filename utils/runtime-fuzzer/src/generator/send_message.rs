// This file is part of Gear.

// Copyright (C) 2021-2024 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use super::GenerationEnvironment;
use crate::runtime;
use gear_call_gen::{GearCall, SendMessageArgs};
use gear_common::Origin;
use gear_core::ids::ProgramId;
use gear_wasm_gen::wasm_gen_arbitrary::{Result, Unstructured};

pub(crate) fn generate(
    unstructured: &mut Unstructured,
    env: GenerationEnvironment,
) -> Result<GearCall> {
    let GenerationEnvironment {
        mut existing_programs,
        max_gas,
        ..
    } = env;
    let existing_programs = {
        if existing_programs.is_empty() {
            // If no existing programs, then send message from program to Alice.
            existing_programs.insert(ProgramId::from_origin(runtime::alice().into_origin()));
        }
        existing_programs.into_iter().collect::<Vec<_>>()
    };
    let program_id = unstructured.choose(&existing_programs).copied()?;
    let payload = super::arbitrary_payload(unstructured)?;
    log::trace!(
        "Random data after payload (send_message) gen {}",
        unstructured.len()
    );
    log::trace!("Payload (send_message) length {:?}", payload.len());

    Ok(SendMessageArgs((program_id, payload, max_gas, 0)).into())
}
