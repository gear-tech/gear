use super::{report::BatchReporter, Seed};
<<<<<<< HEAD
pub use create_program::CreateProgramArgs;
=======
>>>>>>> 736fe4a2 (Introduce send_message task, restructure `batch` module)
pub use send_message::SendMessageArgs;
pub use upload_code::UploadCodeArgs;
pub use upload_program::UploadProgramArgs;

<<<<<<< HEAD
mod create_program;
=======
>>>>>>> 736fe4a2 (Introduce send_message task, restructure `batch` module)
mod send_message;
mod upload_code;
mod upload_program;

pub enum Batch {
    UploadProgram(Vec<UploadProgramArgs>),
    UploadCode(Vec<UploadCodeArgs>),
    SendMessage(Vec<SendMessageArgs>),
<<<<<<< HEAD
    CreateProgram(Vec<CreateProgramArgs>),
=======
>>>>>>> 736fe4a2 (Introduce send_message task, restructure `batch` module)
}

pub struct BatchWithSeed {
    seed: Seed,
    batch: Batch,
}

impl From<BatchWithSeed> for Batch {
    fn from(other: BatchWithSeed) -> Self {
        other.batch
    }
}

impl From<(Seed, Batch)> for BatchWithSeed {
    fn from((seed, batch): (Seed, Batch)) -> Self {
        Self { seed, batch }
    }
}

impl From<BatchWithSeed> for (Seed, Batch) {
    fn from(BatchWithSeed { seed, batch }: BatchWithSeed) -> Self {
        (seed, batch)
    }
}

impl BatchReporter for BatchWithSeed {
    fn report(&self) -> Vec<String> {
        let mut report: Vec<_>;

        match &self.batch {
            Batch::UploadProgram(args) => {
                report = Vec::with_capacity(args.len() + 1);

                report.push(format!(
                    "Batch of `upload_program` with seed {}:",
                    self.seed
                ));

                for (i, UploadProgramArgs((code, salt, payload, gas_limit, value))) in
                    args.iter().enumerate()
                {
                    report.push(format!(
                        "[#{:<2}] code: '0x{}', salt: '0x{}', payload: '0x{}', gas_limit: '{}', value: '{}'",
                        i + 1,
                        hex::encode(code),
                        hex::encode(salt),
                        hex::encode(payload),
                        gas_limit,
                        value
                    ))
                }
            }
            Batch::UploadCode(args) => {
                report = Vec::with_capacity(args.len() + 1);

                report.push(format!("Batch of `upload_code` with seed {}:", self.seed));

                for (i, UploadCodeArgs(code)) in args.iter().enumerate() {
                    report.push(format!("[#{:<2}] code: '0x{}'", i + 1, hex::encode(code)))
                }
            }
            Batch::SendMessage(args) => {
                report = Vec::with_capacity(args.len() + 1);

                report.push(format!("Batch of `send_message` with seed {}:", self.seed));

                for (i, SendMessageArgs((destination, payload, gas_limit, value))) in
                    args.iter().enumerate()
                {
                    report.push(format!(
                        "[#{:<2}] destination: '{}', payload: '0x{}', gas_limit: '{}', value: '{}'",
                        i + 1,
                        destination,
                        hex::encode(payload),
                        gas_limit,
                        value
                    ))
                }
            }
            Batch::CreateProgram(args) => {
                report = Vec::with_capacity(args.len() + 1);

                report.push(format!(
                    "Batch of `create_program` with seed {}:",
                    self.seed
                ));

                for (i, CreateProgramArgs((code, salt, payload, gas_limit, value))) in
                    args.iter().enumerate()
                {
                    report.push(format!(
                        "[#{:<2}] code id: '{}', salt: '0x{}', payload: '0x{}', gas_limit: '{}', value: '{}'",
                        i + 1,
                        code,
                        hex::encode(salt),
                        hex::encode(payload),
                        gas_limit,
                        value
                    ))
                }
            }
        }

        report
    }
}
