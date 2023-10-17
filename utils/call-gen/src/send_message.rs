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

use crate::{
    impl_convert_traits, CallGenRng, GearWasmGenConfigsBundle, GeneratableCallArgs, NamedCallArgs,
    Seed,
};
use gear_core::ids::ProgramId;
use gear_utils::{NonEmpty, RingGet};

// destination, payload, gas, value
type SendMessageArgsInner = (ProgramId, Vec<u8>, u64, u128);

/// Send message args
///
/// Main type used to generate arguments for the `pallet_gear::Pallet::<T>::send_message` call.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct SendMessageArgs(pub SendMessageArgsInner);

impl_convert_traits!(
    SendMessageArgs,
    SendMessageArgsInner,
    SendMessage,
    "send_message"
);

impl GeneratableCallArgs for SendMessageArgs {
    type FuzzerArgs = (NonEmpty<ProgramId>, Seed);
    type ConstArgs<C: GearWasmGenConfigsBundle> = (u64,);

    /// Generates `pallet_gear::Pallet::<T>::send_message` call arguments.
    fn generate<Rng: CallGenRng, Config>(
        (existing_programs, rng_seed): Self::FuzzerArgs,
        (gas_limit,): Self::ConstArgs<()>,
    ) -> Self {
        let mut rng = Rng::seed_from_u64(rng_seed);

        let program_idx = rng.next_u64() as usize;
        let &destination = existing_programs.ring_get(program_idx);

        let mut payload = vec![0; rng.gen_range(1..=100)];
        rng.fill_bytes(&mut payload);

        let name = Self::name();
        log::debug!(
            "Generated `{name}` call with destination = {destination}, payload = {}",
            hex::encode(&payload)
        );

        // TODO #2203
        let value = 0;

        Self((destination, payload, gas_limit, value))
    }
}
