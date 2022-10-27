//! Upload code task.

use crate::{
    batch_pool::{generators, Seed},
    utils::LoaderRng,
};
use gclient::Result as GClientResult;
use gear_core::ids::CodeId;
use primitive_types::H256;

pub type UploadCodeBatchOutput = (Vec<GClientResult<CodeId>>, H256);

pub struct UploadCodeArgs(pub Vec<u8>);

impl From<UploadCodeArgs> for Vec<u8> {
    fn from(UploadCodeArgs(code): UploadCodeArgs) -> Self {
        code
    }
}

impl UploadCodeArgs {
    pub fn generate<Rng: LoaderRng>(code_seed: Seed) -> Self {
        let code = generators::generate_gear_program::<Rng>(code_seed);
        tracing::debug!("Generated `upload_code` with code from seed = {code_seed}");

        Self(code)
    }
}
