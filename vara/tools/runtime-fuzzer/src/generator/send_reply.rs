// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use super::{AUXILIARY_SIZE, GAS_SIZE, ID_SIZE, MAX_PAYLOAD_SIZE, RuntimeStateView, VALUE_SIZE};
use gear_call_gen::{GearCall, SendReplyArgs};
use gear_core::ids::MessageId;
use gear_utils::NonEmpty;
use gear_wasm_gen::wasm_gen_arbitrary::{Result, Unstructured};
use runtime_primitives::Balance;
use std::result::Result as StdResult;

pub(crate) type SendReplyRuntimeData<'a> = (&'a NonEmpty<MessageId>, u64, Balance);

pub(super) const fn data_requirement() -> usize {
    ID_SIZE + MAX_PAYLOAD_SIZE + GAS_SIZE + VALUE_SIZE + AUXILIARY_SIZE
}

impl<'a> TryFrom<RuntimeStateView<'a>> for SendReplyRuntimeData<'a> {
    type Error = ();

    fn try_from(env: RuntimeStateView<'a>) -> StdResult<Self, Self::Error> {
        Ok((env.mailbox.ok_or(())?, env.max_gas, env.current_balance))
    }
}

pub(crate) fn generate(
    unstructured: &mut Unstructured,
    (mailbox, gas, current_balance): SendReplyRuntimeData,
) -> Result<GearCall> {
    log::trace!(
        "Random data before payload (send_reply) gen {}",
        unstructured.len()
    );

    let mid = {
        let random_idx = unstructured.int_in_range(0..=mailbox.len() - 1)?;
        mailbox
            .get(random_idx)
            .copied()
            .expect("idx is checked; qed.")
    };

    let payload = super::arbitrary_payload(unstructured)?;
    log::trace!(
        "Random data after payload (send_reply) gen {}",
        unstructured.len()
    );
    log::trace!("Payload (send_reply) length {:?}", payload.len());

    let value = super::arbitrary_value(unstructured, current_balance)?;
    log::trace!("Random data after value generation {}", unstructured.len());
    log::trace!("Sending value (send_reply) - {value}");

    Ok(SendReplyArgs((mid, payload, gas, value)).into())
}
