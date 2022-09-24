use crate::{
    args::SeedVariant,
    batch_pool::{
        context::TasksContext,
        gear_client,
        report::TaskReporter,
        task::{self, Task},
        Seed,
    },
    generators,
    utils::{self, LoaderRng, LoaderRngCore},
};
use arbitrary::Unstructured;
use rand::RngCore;

pub(crate) fn get_some_seed_generator<Rng: LoaderRng>(
    code_seed_type: Option<SeedVariant>,
) -> Box<dyn LoaderRngCore> {
    match code_seed_type {
        None => Box::new(Rng::seed_from_u64(utils::now())) as _,
        Some(SeedVariant::Dynamic(v)) => Box::new(Rng::seed_from_u64(v)) as _,
        Some(SeedVariant::Constant(v)) => Box::new(ConstantGenerator::new(v)) as _,
    }
}

pub(crate) fn generate_gear_program<Rng: LoaderRng>(seed: Seed) -> Vec<u8> {
    let mut rng = Rng::seed_from_u64(seed);
    let mut buf = vec![0; 100_000];
    rng.fill_bytes(&mut buf);
    let mut u = Unstructured::new(&buf);
    gear_wasm_gen::gen_gear_program_code(&mut u, gear_wasm_gen::GearConfig::default())
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ConstantGenerator(u64);

impl ConstantGenerator {
    pub(crate) fn new(v: u64) -> Self {
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

// Todo DN maybe remove?
pub(super) trait BatchGenerator {
    type Task: Into<gear_client::GearClientCall> + TaskReporter;

    fn generate(&mut self) -> (Seed, Vec<Self::Task>);
}

pub(super) struct BatchGeneratorImpl<Rng: LoaderRng> {
    pub(super) batch_gen_rng: Rng,
    pub(super) batch_size: usize,
    pub(super) _context: TasksContext,
    code_seed_gen: Box<dyn LoaderRngCore>,
}

impl<Rng: LoaderRng> BatchGeneratorImpl<Rng> {
    pub(super) fn new(
        seed: Seed,
        batch_size: usize,
        context: TasksContext,
        code_seed_type: Option<SeedVariant>,
    ) -> Self {
        Self {
            batch_gen_rng: Rng::seed_from_u64(seed),
            batch_size,
            _context: context,
            code_seed_gen: generators::get_some_seed_generator::<Rng>(code_seed_type),
        }
    }
}

impl<Rng: LoaderRng> BatchGenerator for BatchGeneratorImpl<Rng> {
    type Task = Task;

    fn generate(&mut self) -> (Seed, Vec<Self::Task>) {
        let batch_seed = self.batch_gen_rng.next_u64();
        let mut batch = Vec::with_capacity(self.batch_size);
        let mut batch_rng = Rng::seed_from_u64(batch_seed);
        while batch.len() != batch.capacity() {
            let task = match batch_rng.gen_range(0u8..1) {
                0 => task::upload_program_task::<Rng>(
                    self.code_seed_gen.next_u64(),
                    batch_rng.next_u64(),
                ),
                1 => task::upload_code_task::<Rng>(self.code_seed_gen.next_u64()),
                2..=u8::MAX => unreachable!("Num of generators exhausted."),
            };
            batch.push(task);
        }
        (batch_seed, batch)
    }
}
