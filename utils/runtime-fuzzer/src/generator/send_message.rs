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

use super::RuntimeStateView;
use gear_call_gen::{GearCall, SendMessageArgs};
use gear_core::ids::ProgramId;
use gear_utils::NonEmpty;
use gear_wasm_gen::wasm_gen_arbitrary::{Result, Unstructured};
use std::result::Result as StdResult;

pub(crate) type SendMessageRuntimeData<'a> = (NonEmpty<&'a ProgramId>, u64);

impl<'a> TryFrom<RuntimeStateView<'a>> for SendMessageRuntimeData<'a> {
    type Error = ();

    fn try_from(env: RuntimeStateView<'a>) -> StdResult<Self, Self::Error> {
        let programs = NonEmpty::from_slice(&env.programs).ok_or(())?;

        Ok((programs, env.max_gas))
    }
}

pub(crate) fn generate(
    unstructured: &mut Unstructured,
    (programs, gas): SendMessageRuntimeData,
) -> Result<GearCall> {
    let program_id = {
        let random_idx = unstructured.int_in_range(0..=programs.len())?;
        programs
            .get(random_idx)
            .copied()
            .expect("idx is checked; qed.")
    };
    let payload = super::arbitrary_payload(unstructured)?;
    log::trace!(
        "Random data after payload (send_message) gen {}",
        unstructured.len()
    );
    log::trace!("Payload (send_message) length {:?}", payload.len());

    Ok(SendMessageArgs((*program_id, payload, gas, 0)).into())
}
