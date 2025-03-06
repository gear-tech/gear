// This file is part of Gear.

// Copyright (C) 2021-2025 Gear Technologies Inc.
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

use super::{AUXILIARY_SIZE, ID_SIZE, RuntimeStateView};
use gear_call_gen::{ClaimValueArgs, GearCall};
use gear_core::ids::MessageId;
use gear_utils::NonEmpty;
use gear_wasm_gen::wasm_gen_arbitrary::{Result, Unstructured};
use std::result::Result as StdResult;

pub(crate) type ClaimValueRuntimeData<'a> = (&'a NonEmpty<MessageId>,);

pub(super) const fn data_requirement() -> usize {
    ID_SIZE + AUXILIARY_SIZE
}

impl<'a> TryFrom<RuntimeStateView<'a>> for ClaimValueRuntimeData<'a> {
    type Error = ();

    fn try_from(env: RuntimeStateView<'a>) -> StdResult<Self, Self::Error> {
        env.mailbox.map(|mailbox| (mailbox,)).ok_or(())
    }
}

pub(crate) fn generate(
    unstructured: &mut Unstructured,
    (mailbox,): ClaimValueRuntimeData,
) -> Result<GearCall> {
    log::trace!("Generating claim_value call");

    let random_idx = unstructured.int_in_range(0..=mailbox.len() - 1)?;
    mailbox
        .get(random_idx)
        .map(|mid| ClaimValueArgs(*mid).into())
        .ok_or_else(|| unreachable!("idx is checked, qed."))
}
