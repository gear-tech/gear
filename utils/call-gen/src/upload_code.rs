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

//! Upload code args generator.

use crate::{impl_convert_traits, Args, ArgsName, CallGenRng, GearProgGenConfig, Seed};
use gear_core::ids::ProgramId;

/// Upload code args
///
/// Main type used to generate arguments for the `pallet_gear::Pallet::<T>::upload_code` call.
#[derive(Debug, Clone)]
pub struct UploadCodeArgs(pub Vec<u8>);

impl_convert_traits!(UploadCodeArgs, Vec<u8>, UploadCode, "upload_code");

impl Args for UploadCodeArgs {
    type FuzzerArgs = (Vec<ProgramId>, Seed);
    type ConstArgs = (GearProgGenConfig,);

    /// Generates `pallet_gear::Pallet::<T>::upload_code` call arguments.
    fn generate<Rng: CallGenRng>(
        (existing_programs, code_seed): Self::FuzzerArgs,
        (config,): Self::ConstArgs,
    ) -> Self {
        let code = crate::generate_gear_program::<Rng>(code_seed, config, existing_programs);

        let name = Self::name();
        log::debug!("Generated `{name}` with code from seed = {code_seed}");

        Self(code)
    }
}
