// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Upload code args generator.

use crate::{
    CallGenRng, GearWasmGenConfigsBundle, GeneratableCallArgs, NamedCallArgs, Seed,
    impl_convert_traits,
};

/// Upload code args
///
/// Main type used to generate arguments for the `pallet_gear::Pallet::<T>::upload_code` call.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct UploadCodeArgs(pub Vec<u8>);

impl_convert_traits!(UploadCodeArgs, Vec<u8>, UploadCode, "upload_code");

impl GeneratableCallArgs for UploadCodeArgs {
    type FuzzerArgs = Seed;
    type ConstArgs<C: GearWasmGenConfigsBundle> = (C,);

    /// Generates `pallet_gear::Pallet::<T>::upload_code` call arguments.
    fn generate<Rng: CallGenRng, Config: GearWasmGenConfigsBundle>(
        code_seed: Self::FuzzerArgs,
        (config,): Self::ConstArgs<Config>,
    ) -> Self {
        let code = crate::generate_gear_program::<Rng, _>(code_seed, config);

        let name = Self::name();
        log::debug!("Generated `{name}` with code from seed = {code_seed}");

        Self(code)
    }
}
