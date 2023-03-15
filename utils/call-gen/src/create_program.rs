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

//! Create program call generator.

use crate::{CallGenRng, GearCall, GearCallConversionError, Seed};
use gear_core::ids::CodeId;
use gear_utils::{NonEmpty, RingGet};

// code id, salt, payload, gas limit, value
type CreateProgramArgsInner = (CodeId, Vec<u8>, Vec<u8>, u64, u128);

/// Create program args
///
/// Main type used to generate arguments for the `pallet_gear::Pallet::<T>::create_program` call.
#[derive(Debug, Clone)]
pub struct CreateProgramArgs(pub CreateProgramArgsInner);

impl From<CreateProgramArgs> for CreateProgramArgsInner {
    fn from(args: CreateProgramArgs) -> Self {
        args.0
    }
}

impl From<CreateProgramArgs> for GearCall {
    fn from(args: CreateProgramArgs) -> Self {
        GearCall::CreateProgram(args)
    }
}

impl TryFrom<GearCall> for CreateProgramArgs {
    type Error = GearCallConversionError;

    fn try_from(call: GearCall) -> Result<Self, Self::Error> {
        if let GearCall::CreateProgram(call) = call {
            Ok(call)
        } else {
            Err(GearCallConversionError("create_program"))
        }
    }
}

impl CreateProgramArgs {
    /// Generates `pallet_gear::Pallet::<T>::create_program` call arguments.
    pub fn generate<Rng: CallGenRng>(
        existing_codes: NonEmpty<CodeId>,
        rng_seed: Seed,
        gas_limit: u64,
    ) -> Self {
        let mut rng = Rng::seed_from_u64(rng_seed);

        let code_idx = rng.next_u64() as usize;
        let &code = existing_codes.ring_get(code_idx);

        let mut salt = vec![0; rng.gen_range(1..=100)];
        rng.fill_bytes(&mut salt);

        let mut payload = vec![0; rng.gen_range(1..=100)];
        rng.fill_bytes(&mut payload);

        log::debug!(
            "Generated `create_program` call with code id = {code}, salt = {} payload = {}",
            hex::encode(&salt),
            hex::encode(&payload)
        );

        // TODO #2203
        let value = 0;

        Self((code, salt, payload, gas_limit, value))
    }
}
