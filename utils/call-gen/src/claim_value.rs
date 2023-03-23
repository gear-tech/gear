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

//! Claim value args generator.

use crate::{CallGenRng, GearCall, GearCallConversionError, Seed};
use gear_core::ids::MessageId;
use gear_utils::{NonEmpty, RingGet};

/// Claim value args
///
/// Main type used to generate arguments for the `pallet_gear::Pallet::<T>::claim_value` call.
#[derive(Debug, Clone)]
pub struct ClaimValueArgs(pub MessageId);

impl From<ClaimValueArgs> for MessageId {
    fn from(args: ClaimValueArgs) -> Self {
        args.0
    }
}

impl From<ClaimValueArgs> for GearCall {
    fn from(args: ClaimValueArgs) -> Self {
        GearCall::ClaimValue(args)
    }
}

impl TryFrom<GearCall> for ClaimValueArgs {
    type Error = GearCallConversionError;

    fn try_from(call: GearCall) -> Result<Self, Self::Error> {
        if let GearCall::ClaimValue(call) = call {
            Ok(call)
        } else {
            Err(GearCallConversionError("claim_value"))
        }
    }
}

impl ClaimValueArgs {
    /// Generates `pallet_gear::Pallet::<T>::claim_value` call arguments.
    pub fn generate<Rng: CallGenRng>(mailbox: NonEmpty<MessageId>, rng_seed: Seed) -> Self {
        let mut rng = Rng::seed_from_u64(rng_seed);

        let message_idx = rng.next_u64() as usize;
        let &claim_from = mailbox.ring_get(message_idx);

        log::debug!("Generated `claim_value` call with message id = {claim_from}");

        Self(claim_from)
    }
}
