use std::sync::{Arc, Mutex};

use gear_program::api::{generated::api::gear::calls::UploadProgram, Api};
use rand::{RngCore, rngs::SmallRng};

use crate::{
    reporter::{Reporter, SomeReporter, StdoutReporter},
    task::generators,
};

use super::generators::{TaskGen, FutureSomeReporter};

pub(crate) struct UploadProgramTaskGen {
    gear_api: Api,
    code_seed_gen: Arc<Mutex<dyn RngCore + Send + Sync>>,
}

impl UploadProgramTaskGen {
    pub(super) fn try_new(gear_api: Api, code_seed_gen: Arc<Mutex<Box<dyn RngCore + Send + Sync>>> ) -> Self {
        Self {
            code_seed_gen,
            gear_api,
        }
    }
}

impl TaskGen for UploadProgramTaskGen {
    type Output = FutureSomeReporter;
    fn gen(&self) -> Self::Output {
        let seed = self
            .code_seed_gen
            .lock()
            .expect("code seed generator panic")
            .next_u64();

        Box::pin(upload_program_task(self.gear_api.clone(), seed))
    }
}

async fn upload_program_task(gear_api: Api, code_gen_seed: u64) -> SomeReporter {
    // todo avoid panics
    let signer = gear_api.try_signer(None).unwrap();

    let mut reporter = StdoutReporter::new(code_gen_seed);

    let code = generators::generate_gear_program::<SmallRng>(code_gen_seed);
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
