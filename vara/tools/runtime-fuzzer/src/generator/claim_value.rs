// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

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
