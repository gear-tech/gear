//! Upload program task

use crate::{
    batch_pool::{generators, Seed},
    utils::LoaderRng,
};

pub type UploadProgramArgsInner = (Vec<u8>, Vec<u8>, Vec<u8>, u64, u128);

pub struct UploadProgramArgs(pub UploadProgramArgsInner);

impl From<UploadProgramArgs> for UploadProgramArgsInner {
    fn from(UploadProgramArgs((code, salt, payload, gas_limit, value)): UploadProgramArgs) -> Self {
        (code, salt, payload, gas_limit, value)
    }
}

impl UploadProgramArgs {
    pub fn generate<Rng: LoaderRng>(code_seed: Seed, rng_seed: Seed) -> Self {
        let mut rng = Rng::seed_from_u64(rng_seed);

        let code = generators::generate_gear_program::<Rng>(code_seed);

        let mut salt = vec![0; rng.gen_range(1..=100)];
        rng.fill_bytes(&mut salt);

        let mut payload = vec![0; rng.gen_range(1..=100)];
        rng.fill_bytes(&mut payload);

        tracing::debug!(
            "Generated `upload_program` batch with code seed = {code_seed}, salt = {} payload = {}",
            hex::encode(&salt),
            hex::encode(&payload)
        );

        let gas_limit = 240_000_000_000;

        let value = 0;

        Self((code, salt, payload, gas_limit, value))
    }
}
