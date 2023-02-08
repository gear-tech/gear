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

//! Send message args generator.

use crate::{CallGenRng, GearCall, Seed};
use gear_core::{
    ids::ProgramId,
    utils::{NonEmpty, RingGet},
};

// destination, payload, gas, value
type SendMessageArgsInner = (ProgramId, Vec<u8>, u64, u128);
 
/// Send message args
///
/// Main type used to generate arguments for the `pallet_gear::Pallet::<T>::send_message` call.
pub struct SendMessageArgs(pub SendMessageArgsInner);

impl From<SendMessageArgs> for SendMessageArgsInner {
    fn from(SendMessageArgs((destination, payload, gas_limit, value)): SendMessageArgs) -> Self {
        (destination, payload, gas_limit, value)
    }
}

// TODO [sab] use macros for From and TryFrom calls
impl From<SendMessageArgs> for GearCall {
    fn from(args: SendMessageArgs) -> Self {
        GearCall::SendMessage(args)
    }
}
    
impl TryFrom<GearCall> for SendMessageArgs {
    type Error = ();

    fn try_from(call: GearCall) -> Result<Self, Self::Error> {
        if let GearCall::SendMessage(call) = call {
            Ok(call)
        } else {
            Err(())
        }
    }
}

/// Send message args
///
/// Main type used to generate arguments for the `pallet_gear::Pallet::<T>::send_message` call.
impl SendMessageArgs {
    /// Generates `pallet_gear::Pallet::<T>::send_message` call arguments.
    pub fn generate<Rng: CallGenRng>(
        existing_programs: NonEmpty<ProgramId>,
        rng_seed: Seed,
        gas_limit: u64,
    ) -> Self {
        let mut rng = Rng::seed_from_u64(rng_seed);

        let program_idx = rng.next_u64() as usize;
        let &destination = existing_programs.ring_get(program_idx);

        let mut payload = vec![0; rng.gen_range(1..=100)];
        rng.fill_bytes(&mut payload);

        log::debug!(
            "Generated `send_message` batch with destination = {destination}, payload = {}",
            hex::encode(&payload)
        );

        let value = 0;

        Self((destination, payload, gas_limit, value))
    }
}
