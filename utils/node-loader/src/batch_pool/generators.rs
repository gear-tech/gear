use crate::{
    args::SeedVariant,
    batch_pool::{batch::Batch, context::Context, Seed},
    generators,
    utils::{self, LoaderRng, LoaderRngCore},
};
use arbitrary::Unstructured;
use rand::RngCore;

use super::batch::{BatchWithSeed, UploadProgramArgs, UploadCodeArgs};

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
    pub _context: Context,
    code_seed_gen: Box<dyn LoaderRngCore>,
}

impl<Rng: LoaderRng> BatchGenerator<Rng> {
    pub fn new(
        seed: Seed,
        batch_size: usize,
        context: Context,
        code_seed_type: Option<SeedVariant>,
    ) -> Self {
        Self {
            batch_gen_rng: Rng::seed_from_u64(seed),
            batch_size,
            _context: context,
            code_seed_gen: generators::get_some_seed_generator::<Rng>(code_seed_type),
        }
    }

    pub fn generate(&mut self) -> BatchWithSeed {
        let seed = self.batch_gen_rng.next_u64();
        let mut rng = Rng::seed_from_u64(seed);

        let spec = rng.next_u64();

        let batch = match spec % 2 {
            0 => Batch::UploadProgram(
                (0..self.batch_size)
                    .map(|_| {
                        UploadProgramArgs::generate::<Rng>(
                            self.code_seed_gen.next_u64(),
                            rng.next_u64(),
                        )
                    })
                    .collect(),
            ),
            1 => Batch::UploadCode(
                (0..self.batch_size)
                    .map(|_| UploadCodeArgs::generate::<Rng>(self.code_seed_gen.next_u64()))
                    .collect(),
            ),
            _ => unreachable!(),
        };

        (seed, batch).into()
    }
}
