//! Send message task

use crate::{
    batch_pool::Seed,
    utils::{LoaderRng, NonEmptyVec, RingGet},
};
use gear_core::ids::ProgramId;

pub type SendMessageArgsInner = (ProgramId, Vec<u8>, u64, u128);

pub struct SendMessageArgs(pub SendMessageArgsInner);

impl From<SendMessageArgs> for SendMessageArgsInner {
    fn from(SendMessageArgs((destination, payload, gas_limit, value)): SendMessageArgs) -> Self {
        (destination, payload, gas_limit, value)
    }
}

impl SendMessageArgs {
    pub fn generate<Rng: LoaderRng>(
        existing_programs: NonEmptyVec<ProgramId>,
        rng_seed: Seed,
    ) -> Self {
        let mut rng = Rng::seed_from_u64(rng_seed);

        let program_idx = rng.next_u64() as usize;
        let destination = existing_programs
            .ring_get(program_idx)
            .copied()
            .expect("Infallible");

        let mut payload = vec![0; rng.gen_range(1..=100)];
        rng.fill_bytes(&mut payload);

        // TODO: add this.
        let gas_limit = 240_000_000_000;

        // TODO: add this.
        let value = 0;

        Self((destination, payload, gas_limit, value))
    }
}
