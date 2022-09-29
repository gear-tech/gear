use super::{generators, report::BatchReporter, Seed};
use crate::utils::LoaderRng;

pub struct UploadProgramArgs(Vec<u8>, Vec<u8>, Vec<u8>, u64, u128);

impl From<UploadProgramArgs> for (Vec<u8>, Vec<u8>, Vec<u8>, u64, u128) {
    fn from(UploadProgramArgs(code, salt, payload, gas_limit, value): UploadProgramArgs) -> Self {
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

        // TODO: add this.
        let gas_limit = 240_000_000_000;

        // TODO: add this.
        let value = 0;

        Self(code, salt, payload, gas_limit, value)
    }
}

pub struct UploadCodeArgs(Vec<u8>);

impl From<UploadCodeArgs> for Vec<u8> {
    fn from(UploadCodeArgs(code): UploadCodeArgs) -> Self {
        code
    }
}

impl UploadCodeArgs {
    pub fn generate<Rng: LoaderRng>(code_seed: Seed) -> Self {
        let code = generators::generate_gear_program::<Rng>(code_seed);

        Self(code)
    }
}

pub enum Batch {
    UploadProgram(Vec<UploadProgramArgs>),
    UploadCode(Vec<UploadCodeArgs>),
    // SendMessage,
}

impl From<BatchWithSeed> for Batch {
    fn from(other: BatchWithSeed) -> Self {
        other.batch
    }
}

pub struct BatchWithSeed {
    seed: Seed,
    batch: Batch,
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

                for (i, UploadProgramArgs(code, salt, payload, gas_limit, value)) in
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
        }

        report
    }
}
