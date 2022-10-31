//! Send message task

use crate::{
    batch_pool::Seed,
    utils::{LoaderRng, NonEmptyVec},
};
use gclient::Result;
use gear_core::ids::{MessageId, ProgramId};
use primitive_types::H256;

pub type SendMessageArgsInner = (ProgramId, Vec<u8>, u64, u128);
pub type SendMessageBatchOutput = (Vec<Result<(MessageId, ProgramId)>>, H256);

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
        gas_limit: u64,
    ) -> Self {
        let mut rng = Rng::seed_from_u64(rng_seed);

        let program_idx = rng.next_u64() as usize;
        let &destination = existing_programs.ring_get(program_idx);

        let mut payload = vec![0; rng.gen_range(1..=100)];
        rng.fill_bytes(&mut payload);

        tracing::debug!(
            "Generated `send_message` batch with destination = {destination}, payload = {}",
            hex::encode(&payload)
        );

        let value = 0;

        Self((destination, payload, gas_limit, value))
    }
}
