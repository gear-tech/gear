//! Create program task

use crate::{
    batch_pool::Seed,
    utils::{LoaderRng, NonEmptyVec},
};
use gclient::Result;
use gear_core::ids::{CodeId, MessageId, ProgramId};
use primitive_types::H256;

pub type CreateProgramArgsInner = (CodeId, Vec<u8>, Vec<u8>, u64, u128);
pub type CreateProgramBatchOutput = (Vec<Result<(MessageId, ProgramId)>>, H256);

pub struct CreateProgramArgs(pub CreateProgramArgsInner);

impl From<CreateProgramArgs> for CreateProgramArgsInner {
    fn from(
        CreateProgramArgs((code_id, salt, payload, gas_limit, value)): CreateProgramArgs,
    ) -> Self {
        (code_id, salt, payload, gas_limit, value)
    }
}

impl CreateProgramArgs {
    pub fn generate<Rng: LoaderRng>(
        existing_codes: NonEmptyVec<CodeId>,
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

        tracing::debug!(
            "Generated `create_program` batch with code id = {code}, salt = {} payload = {}",
            hex::encode(&salt),
            hex::encode(&payload)
        );

        let value = 0;

        Self((code, salt, payload, gas_limit, value))
    }
}
