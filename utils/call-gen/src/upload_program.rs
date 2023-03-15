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

//! Upload program args generator.

use crate::{CallGenRng, GearCall, GearCallConversionError, GearProgGenConfig, Seed};
use gear_core::ids::ProgramId;

// code, salt, payload, gas, value
type UploadProgramArgsInner = (Vec<u8>, Vec<u8>, Vec<u8>, u64, u128);

/// Upload program args
///
/// Main type used to generate arguments for the `pallet_gear::Pallet::<T>::upload_program` call.
#[derive(Debug, Clone)]
pub struct UploadProgramArgs(pub UploadProgramArgsInner);

impl From<UploadProgramArgs> for UploadProgramArgsInner {
    fn from(args: UploadProgramArgs) -> Self {
        args.0
    }
}

impl From<UploadProgramArgs> for GearCall {
    fn from(args: UploadProgramArgs) -> Self {
        GearCall::UploadProgram(args)
    }
}

impl TryFrom<GearCall> for UploadProgramArgs {
    type Error = GearCallConversionError;

    fn try_from(call: GearCall) -> Result<Self, Self::Error> {
        if let GearCall::UploadProgram(call) = call {
            Ok(call)
        } else {
            Err(GearCallConversionError("upload_program"))
        }
    }
}

impl UploadProgramArgs {
    /// Generates `pallet_gear::Pallet::<T>::upload_program` call arguments.
    pub fn generate<Rng: CallGenRng>(
        code_seed: Seed,
        rng_seed: Seed,
        gas_limit: u64,
        config: GearProgGenConfig,
        programs: Vec<ProgramId>,
    ) -> Self {
        let mut rng = Rng::seed_from_u64(rng_seed);

        let code = crate::generate_gear_program::<Rng>(code_seed, config, programs);

        let mut salt = vec![0; rng.gen_range(1..=100)];
        rng.fill_bytes(&mut salt);

        let mut payload = vec![0; rng.gen_range(1..=100)];
        rng.fill_bytes(&mut payload);

        log::debug!(
            "Generated `upload_program` call with code seed = {code_seed}, salt = {}, payload = {}",
            hex::encode(&salt),
            hex::encode(&payload)
        );

        // TODO #2203
        let value = 0;

        Self((code, salt, payload, gas_limit, value))
    }
}
