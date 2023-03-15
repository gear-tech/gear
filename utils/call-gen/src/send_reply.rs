// This file is part of Gear.

// Copyright (C) 2021-2023 Gear Technologies Inc.
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

//! Send reply args generator.

use crate::{CallGenRng, GearCall, GearCallConversionError, Seed};
use gear_core::ids::MessageId;
use gear_utils::{NonEmpty, RingGet};

// reply to message id, payload, gas limit, value
type SendReplyArgsInner = (MessageId, Vec<u8>, u64, u128);

/// Send reply args
///
/// Main type used to generate arguments for the `pallet_gear::Pallet::<T>::send_reply` call.
#[derive(Debug, Clone)]
pub struct SendReplyArgs(pub SendReplyArgsInner);

impl From<SendReplyArgs> for SendReplyArgsInner {
    fn from(args: SendReplyArgs) -> Self {
        args.0
    }
}

impl From<SendReplyArgs> for GearCall {
    fn from(args: SendReplyArgs) -> Self {
        GearCall::SendReply(args)
    }
}

impl TryFrom<GearCall> for SendReplyArgs {
    type Error = GearCallConversionError;

    fn try_from(call: GearCall) -> Result<Self, Self::Error> {
        if let GearCall::SendReply(call) = call {
            Ok(call)
        } else {
            Err(GearCallConversionError("send_reply"))
        }
    }
}

impl SendReplyArgs {
    /// Generates `pallet_gear::Pallet::<T>::send_reply` call arguments.
    pub fn generate<Rng: CallGenRng>(
        mailbox: NonEmpty<MessageId>,
        rng_seed: Seed,
        gas_limit: u64,
    ) -> Self {
        let mut rng = Rng::seed_from_u64(rng_seed);

        let message_idx = rng.next_u64() as usize;
        let &message_id = mailbox.ring_get(message_idx);

        let mut payload = vec![0; rng.gen_range(1..=100)];
        rng.fill_bytes(&mut payload);

        log::debug!(
            "Generated `send_reply` call with message id = {message_id}, payload = {}",
            hex::encode(&payload)
        );

        // TODO #2203
        let value = 0;

        Self((message_id, payload, gas_limit, value))
    }
}
