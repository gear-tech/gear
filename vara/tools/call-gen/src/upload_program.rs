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

//! Upload program args generator.

use crate::{
    CallGenRng, GearWasmGenConfigsBundle, GeneratableCallArgs, NamedCallArgs, Seed,
    impl_convert_traits,
};

// code, salt, payload, gas, value
type UploadProgramArgsInner = (Vec<u8>, Vec<u8>, Vec<u8>, u64, u128);

/// Upload program args
///
/// Main type used to generate arguments for the `pallet_gear::Pallet::<T>::upload_program` call.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct UploadProgramArgs(pub UploadProgramArgsInner);

impl_convert_traits!(
    UploadProgramArgs,
    UploadProgramArgsInner,
    UploadProgram,
    "upload_program"
);

impl GeneratableCallArgs for UploadProgramArgs {
    type FuzzerArgs = (Seed, Seed);
    type ConstArgs<C: GearWasmGenConfigsBundle> = (u64, C);

    /// Generates `pallet_gear::Pallet::<T>::upload_program` call arguments.
    fn generate<Rng: CallGenRng, Config: GearWasmGenConfigsBundle>(
        (code_seed, rng_seed): Self::FuzzerArgs,
        (gas_limit, config): Self::ConstArgs<Config>,
    ) -> Self {
        let mut rng = Rng::seed_from_u64(rng_seed);

        let code = crate::generate_gear_program::<Rng, _>(code_seed, config);

        let mut salt = vec![0; rng.gen_range(1..=100)];
        rng.fill_bytes(&mut salt);

        let mut payload = vec![0; rng.gen_range(1..=100)];
        rng.fill_bytes(&mut payload);

        let name = Self::name();
        log::debug!(
            "Generated `{name}` call with code seed = {code_seed}, salt = {}, payload = {}",
            hex::encode(&salt),
            hex::encode(&payload)
        );

        // TODO #2203
        let value = 0;

        Self((code, salt, payload, gas_limit, value))
    }
}
