use futures::Future;
use gear_program::api::{generated::api::gear::calls::UploadProgram, Api};
use rand::RngCore;

use crate::{
    reporter::{Reporter, SomeReporter, StdoutReporter},
    task::generators,
    utils,
};

pub(crate) struct UploadProgramTaskGen {
    gear_api: Api,
    code_seed_gen: Box<dyn RngCore>,
}

impl UploadProgramTaskGen {
    pub(super) fn try_new(gear_api: Api, code_rand_gen: Box<dyn RngCore>) -> Self {
        Self {
            code_seed_gen: code_rand_gen,
            gear_api,
        }
    }

    pub(super) fn gen<Rng: utils::Rng>(
        &mut self,
    ) -> impl Future<Output = SomeReporter> + Send + 'static {
        upload_program_task::<Rng>(self.gear_api.clone(), self.code_seed_gen.next_u64())
    }
}

async fn upload_program_task<Rng: utils::Rng>(gear_api: Api, code_gen_seed: u64) -> SomeReporter {
    // todo avoid panics
    let signer = gear_api.try_signer(None).unwrap();

    let mut reporter = StdoutReporter::new(code_gen_seed);

    let code = generators::generate_gear_program::<Rng>(code_gen_seed);
    let _ = reporter.record(format!("Gen code size = {}", code.len()));

    let payload = UploadProgram {
        code: code.clone(),
        salt: hex::decode("00").unwrap(),
        init_payload: hex::decode("00").unwrap(),
        gas_limit: 250_000_000_000,
        value: 0,
    };

    if let Err(e) = signer.submit_program(payload).await {
        let _ = reporter.record(format!("ERROR: {}", e));
    } else {
        let _ = reporter.record("Successfully receive response".into());
    }

    Box::new(reporter)
}
