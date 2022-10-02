use super::batch::{
    BatchWithSeed, CreateProgramArgs, SendMessageArgs, UploadCodeArgs, UploadProgramArgs,
};
use crate::{
    args::SeedVariant,
    batch_pool::{batch::Batch, context::Context, Seed},
    generators,
    utils::{self, LoaderRng, LoaderRngCore, NonEmptyVec},
};
use arbitrary::Unstructured;
use gear_core::ids::{CodeId, ProgramId};
use rand::RngCore;

pub fn get_some_seed_generator<Rng: LoaderRng>(
    code_seed_type: Option<SeedVariant>,
) -> Box<dyn LoaderRngCore> {
    match code_seed_type {
        None => Box::new(Rng::seed_from_u64(utils::now())) as _,
        Some(SeedVariant::Dynamic(v)) => Box::new(Rng::seed_from_u64(v)) as _,
        Some(SeedVariant::Constant(v)) => Box::new(ConstantGenerator::new(v)) as _,
    }
}

pub fn generate_gear_program<Rng: LoaderRng>(seed: Seed) -> Vec<u8> {
    let mut rng = Rng::seed_from_u64(seed);

    let mut buf = vec![0; 100_000];
    rng.fill_bytes(&mut buf);

    let mut u = Unstructured::new(&buf);

    gear_wasm_gen::gen_gear_program_code(&mut u, gear_wasm_gen::GearConfig::default())
}

#[derive(Debug, Clone, Copy)]
pub struct ConstantGenerator(u64);

impl ConstantGenerator {
    pub fn new(v: u64) -> Self {
        Self(v)
    }
}

impl RngCore for ConstantGenerator {
    fn next_u32(&mut self) -> u32 {
        self.0 as u32
    }

    fn next_u64(&mut self) -> u64 {
        self.0
    }

    fn fill_bytes(&mut self, _dest: &mut [u8]) {
        unimplemented!()
    }

    fn try_fill_bytes(&mut self, _dest: &mut [u8]) -> Result<(), rand::Error> {
        unimplemented!()
    }
}

pub struct BatchGenerator<Rng: LoaderRng> {
    pub batch_gen_rng: Rng,
    pub batch_size: usize,
    code_seed_gen: Box<dyn LoaderRngCore>,
}

impl<Rng: LoaderRng> BatchGenerator<Rng> {
    pub fn new(seed: Seed, batch_size: usize, code_seed_type: Option<SeedVariant>) -> Self {
        Self {
            batch_gen_rng: Rng::seed_from_u64(seed),
            batch_size,
            code_seed_gen: generators::get_some_seed_generator::<Rng>(code_seed_type),
        }
    }

    pub fn generate(&mut self, context: Context) -> BatchWithSeed {
        let seed = self.batch_gen_rng.next_u64();
        let mut rng = Rng::seed_from_u64(seed);

        let spec = rng.next_u64();

        let batch = match spec % 4 {
            0 => self.gen_upload_program_batch(&mut rng),
            1 => self.gen_upload_code_batch(),
            2 => match NonEmptyVec::try_from_iter(context.programs.iter().copied()) {
                Ok(existing_programs) => self.gen_send_message_batch(existing_programs, &mut rng),
                Err(_) => self.gen_upload_program_batch(&mut rng),
            },
            3 => match NonEmptyVec::try_from_iter(context.codes.iter().copied()) {
                Ok(existing_codes) => self.gen_create_program_batch(existing_codes, &mut rng),
                Err(_) => self.gen_upload_program_batch(&mut rng),
            },
            _ => unreachable!(),
        };

        (seed, batch).into()
    }

    fn gen_upload_program_batch(&mut self, rng: &mut Rng) -> Batch {
        let inner = utils::iterator_with_args(self.batch_size, || {
            (self.code_seed_gen.next_u64(), rng.next_u64())
        })
        .map(|(code_seed, rng_seed)| UploadProgramArgs::generate::<Rng>(code_seed, rng_seed))
        .collect();

        Batch::UploadProgram(inner)
    }

    fn gen_upload_code_batch(&mut self) -> Batch {
        let inner = utils::iterator_with_args(self.batch_size, || self.code_seed_gen.next_u64())
            .map(|code_seed| UploadCodeArgs::generate::<Rng>(code_seed))
            .collect();

        Batch::UploadCode(inner)
    }

    fn gen_send_message_batch(
        &mut self,
        existing_programs: NonEmptyVec<ProgramId>,
        rng: &mut Rng,
    ) -> Batch {
        let inner = utils::iterator_with_args(self.batch_size, || {
            (existing_programs.clone(), rng.next_u64())
        })
        .map(|(existing_programs, rng_seed)| {
            SendMessageArgs::generate::<Rng>(existing_programs, rng_seed)
        })
        .collect();

        Batch::SendMessage(inner)
    }

    fn gen_create_program_batch(
        &mut self,
        existing_codes: NonEmptyVec<CodeId>,
        rng: &mut Rng,
    ) -> Batch {
        let inner =
            utils::iterator_with_args(self.batch_size, || (existing_codes.clone(), rng.next_u64()))
                .map(|(existing_programs, rng_seed)| {
                    CreateProgramArgs::generate::<Rng>(existing_programs, rng_seed)
                })
                .collect();

        Batch::CreateProgram(inner)
    }
}
