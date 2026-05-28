// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Create program call generator.

use crate::{
    CallGenRng, GearWasmGenConfigsBundle, GeneratableCallArgs, NamedCallArgs, Seed,
    impl_convert_traits,
};
use gear_core::ids::CodeId;
use gear_utils::{NonEmpty, RingGet};

// code id, salt, payload, gas limit, value
type CreateProgramArgsInner = (CodeId, Vec<u8>, Vec<u8>, u64, u128);

/// Create program args
///
/// Main type used to generate arguments for the `pallet_gear::Pallet::<T>::create_program` call.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct CreateProgramArgs(pub CreateProgramArgsInner);

impl_convert_traits!(
    CreateProgramArgs,
    CreateProgramArgsInner,
    CreateProgram,
    "create_program"
);

impl GeneratableCallArgs for CreateProgramArgs {
    type FuzzerArgs = (NonEmpty<CodeId>, Seed);
    type ConstArgs<C: GearWasmGenConfigsBundle> = (u64,);

    /// Generates `pallet_gear::Pallet::<T>::create_program` call arguments.
    fn generate<Rng: CallGenRng, Config>(
        (existing_codes, rng_seed): Self::FuzzerArgs,
        (gas_limit,): Self::ConstArgs<()>,
    ) -> Self {
        let mut rng = Rng::seed_from_u64(rng_seed);

        let code_idx = rng.next_u64() as usize;
        let &code = existing_codes.ring_get(code_idx);

        let mut salt = vec![0; rng.gen_range(1..=100)];
        rng.fill_bytes(&mut salt);

        let mut payload = vec![0; rng.gen_range(1..=100)];
        rng.fill_bytes(&mut payload);

        let name = Self::name();
        log::debug!(
            "Generated `{name}` call with code id = {code}, salt = {} payload = {}",
            hex::encode(&salt),
            hex::encode(&payload)
        );

        // TODO #2203
        let value = 0;

        Self((code, salt, payload, gas_limit, value))
    }
}
