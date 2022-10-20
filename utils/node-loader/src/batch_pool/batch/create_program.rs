//! Create program task

use gear_core::ids::CodeId;

use crate::{
    batch_pool::Seed,
    utils::{LoaderRng, NonEmptyVec, RingGet},
};

pub type CreateProgramArgsInner = (CodeId, Vec<u8>, Vec<u8>, u64, u128);

pub struct CreateProgramArgs(pub CreateProgramArgsInner);

impl From<CreateProgramArgs> for CreateProgramArgsInner {
    fn from(
        CreateProgramArgs((code_id, salt, payload, gas_limit, value)): CreateProgramArgs,
    ) -> Self {
        (code_id, salt, payload, gas_limit, value)
    }
}

impl CreateProgramArgs {
    pub fn generate<Rng: LoaderRng>(existing_codes: NonEmptyVec<CodeId>, rng_seed: Seed) -> Self {
        let mut rng = Rng::seed_from_u64(rng_seed);

        let code_idx = rng.next_u64() as usize;
        let code = existing_codes
            .ring_get(code_idx)
            .copied()
            .expect("Infallible");

        let mut salt = vec![0; rng.gen_range(1..=100)];
        rng.fill_bytes(&mut salt);

        let mut payload = vec![0; rng.gen_range(1..=100)];
        rng.fill_bytes(&mut payload);

        // TODO: add this.
        let gas_limit = 240_000_000_000;

        // TODO: add this.
        let value = 0;

        Self((code, salt, payload, gas_limit, value))
    }
}
