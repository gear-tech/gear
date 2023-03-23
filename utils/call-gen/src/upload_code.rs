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

use crate::{CallGenRng, GearCall, GearCallConversionError, GearProgGenConfig, Seed};
use gear_core::ids::ProgramId;

/// Upload code args
///
/// Main type used to generate arguments for the `pallet_gear::Pallet::<T>::upload_code` call.
#[derive(Debug, Clone)]
pub struct UploadCodeArgs(pub Vec<u8>);

impl From<UploadCodeArgs> for Vec<u8> {
    fn from(args: UploadCodeArgs) -> Self {
        args.0
    }
}

impl From<UploadCodeArgs> for GearCall {
    fn from(args: UploadCodeArgs) -> Self {
        GearCall::UploadCode(args)
    }
}

impl TryFrom<GearCall> for UploadCodeArgs {
    type Error = GearCallConversionError;

    fn try_from(call: GearCall) -> Result<Self, Self::Error> {
        if let GearCall::UploadCode(call) = call {
            Ok(call)
        } else {
            Err(GearCallConversionError("upload_code"))
        }
    }
}

impl UploadCodeArgs {
    /// Generates `pallet_gear::Pallet::<T>::upload_code` call arguments.
    pub fn generate<Rng: CallGenRng>(
        code_seed: Seed,
        config: GearProgGenConfig,
        programs: Vec<ProgramId>,
    ) -> Self {
        let code = crate::generate_gear_program::<Rng>(code_seed, config, programs);
        log::debug!("Generated `upload_code` with code from seed = {code_seed}");

        Self(code)
    }
}
