use std::sync::{Arc, Mutex};

use gear_program::api::{generated::api::gear::calls::UploadCode, Api};
use rand::RngCore;

use crate::{
    reporter::{Reporter, SomeReporter, StdoutReporter},
    task::generators,
};

use super::generators::{TaskGen, FutureSomeReporter};

pub(crate) struct UploadCodeTaskGen {
    gear_api: Api,
    code_seed_gen: Arc<Mutex<dyn RngCore + Send + Sync>>,
}

impl UploadCodeTaskGen {
    pub(super) fn try_new(gear_api: Api, code_seed_gen: Arc<Mutex<Box<dyn RngCore + Send + Sync>>>) -> Self {
        Self {
            gear_api,
            code_seed_gen,
        }
    }
}

impl<Rng: crate::Rng> TaskGen<Rng> for UploadCodeTaskGen {
    type Output = FutureSomeReporter;
    fn gen(&self) -> Self::Output {
        let seed = self
            .code_seed_gen
            .lock()
            .expect("code seed generator panic")
            .next_u64();
        Box::pin(upload_code_task::<Rng>(self.gear_api.clone(), seed))
    }
}

async fn upload_code_task<Rng: crate::Rng>(gear_api: Api, code_seed_gen: u64) -> SomeReporter {
    let signer = gear_api.try_signer(None).unwrap();

    let mut reporter = StdoutReporter::new(code_seed_gen);

    let code = generators::generate_gear_program::<Rng>(code_seed_gen);
    let _ = reporter.record(format!("Gen code size = {}", code.len()));

    let payload = UploadCode { code: code.clone() };

    if let Err(e) = signer.upload_code(payload).await {
        let _ = reporter.record(format!("ERROR: {}", e));
    } else {
        let _ = reporter.record("Successfully receive response".into());
    }

    Box::new(reporter)
}
