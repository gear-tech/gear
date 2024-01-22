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

use gear_call_gen::{GearCall, SendReplyArgs};
use gear_wasm_gen::wasm_gen_arbitrary::{Result, Unstructured};

use crate::GenerationEnvironment;

pub(crate) fn generate(
    unstructured: &mut Unstructured,
    env: GenerationEnvironment,
) -> Result<Option<GearCall>> {
    log::trace!(
        "Random data before payload (send_reply) gen {}",
        unstructured.len()
    );

    let GenerationEnvironment {
        max_gas, mailbox, ..
    } = env;
    let mailbox = mailbox.into_iter().collect::<Vec<_>>();
    if mailbox.is_empty() {
        return Ok(None);
    }

    let mailbox_mid = unstructured.choose(&mailbox).copied()?;
    let payload = super::arbitrary_payload(unstructured)?;
    log::trace!(
        "Random data after payload (send_reply) gen {}",
        unstructured.len()
    );
    log::trace!("Payload (send_reply) length {:?}", payload.len());

    Ok(Some(
        SendReplyArgs((mailbox_mid, payload, max_gas, 0)).into(),
    ))
}
