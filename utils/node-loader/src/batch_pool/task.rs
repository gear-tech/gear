use super::{context::TasksContext, gear_client, report::TaskReporter};
use crate::utils::LoaderRng;

pub(super) fn upload_program_task<Rng: LoaderRng>(
    task_context: &mut TasksContext,
    rng_seed: u64,
) -> Task {
    let mut rng = Rng::seed_from_u64(rng_seed);
    let code_seed = task_context.code_seed_gen.next_u64();

    let code = crate::generators::generate_gear_program::<Rng>(code_seed);
    let mut salt = vec![0; rng.gen_range(1..=100)];
    rng.fill_bytes(&mut salt);

    let mut payload = vec![0; rng.gen_range(1..=100)];
    rng.fill_bytes(&mut payload);

    Task::UploadProgram {
        code,
        salt,
        payload,
    }
}

pub(super) fn upload_code_task<Rng: LoaderRng>(task_context: &mut TasksContext) -> Task {
    let code_seed = task_context.code_seed_gen.next_u64();

    let code = crate::generators::generate_gear_program::<Rng>(code_seed);

    Task::UploadCode { code }
}

#[derive(Debug)]
pub(super) enum Task {
    UploadProgram {
        code: Vec<u8>,
        salt: Vec<u8>,
        payload: Vec<u8>,
    },
    UploadCode {
        code: Vec<u8>,
    },
    // SendMessage,
}

impl From<Task> for gear_client::GearClientCall {
    fn from(v: Task) -> Self {
        match v {
            Task::UploadProgram { .. } => gear_client::GearClientCall,
            Task::UploadCode { .. } => gear_client::GearClientCall,
        }
    }
}

impl TaskReporter for Task {
    fn report(&self) -> String {
        match self {
            Task::UploadProgram {
                code,
                salt,
                payload,
            } => format!(
                "code - {:?}, salt - {:?} and payload - {:?}.",
                code, salt, payload,
            ),
            Task::UploadCode { code } => format! {
                "code - {:?}", code
            },
        }
    }
}
