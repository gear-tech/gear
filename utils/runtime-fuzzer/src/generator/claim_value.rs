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

use crate::GenerationEnvironment;
use gear_call_gen::{ClaimValueArgs, GearCall};
use gear_wasm_gen::wasm_gen_arbitrary::{Error, Result, Unstructured};

pub(crate) fn generate(
    unstructured: &mut Unstructured,
    env: GenerationEnvironment,
) -> Result<Option<GearCall>> {
    log::trace!("Generating claim_value call");

    let GenerationEnvironment { mailbox, .. } = env;
    let mailbox = mailbox.into_iter().collect::<Vec<_>>();

    unstructured
        .choose(&mailbox)
        .map(|mid| Some(ClaimValueArgs(*mid).into()))
        .or_else(|err| matches!(err, Error::EmptyChoose).then(|| None).ok_or(err))
}
