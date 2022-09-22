use crate::{
    args::SeedVariant,
    batch_pool::{
        context::TasksContext,
        gear_client,
        report::TaskReporter,
        task::{upload_program_task, Task},
    },
    utils,
};
use arbitrary::Unstructured;
use rand::RngCore;
use std::marker::PhantomData;

pub(crate) fn get_some_seed_generator<Rng: crate::LoaderRng>(
    code_seed_type: Option<SeedVariant>,
) -> Box<dyn crate::utils::LoaderRngCore> {
    match code_seed_type {
        None => Box::new(Rng::seed_from_u64(utils::now())) as _,
        Some(SeedVariant::Dynamic(v)) => Box::new(Rng::seed_from_u64(v)) as _,
        Some(SeedVariant::Constant(v)) => Box::new(ConstantGenerator::new(v)) as _,
    }
}

pub(crate) fn generate_gear_program<Rng: crate::LoaderRng>(seed: u64) -> Vec<u8> {
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

    fn batch_size(&self) -> usize;
    fn generate(&mut self) -> Vec<Self::Task>;
    fn seed(&self) -> u64;
}

pub(super) struct BatchGeneratorImpl<Rng: crate::LoaderRng> {
    pub(super) seed: u64,
    pub(super) batch_size: usize,
    pub(super) context: TasksContext,
    _phantom: PhantomData<Rng>,
}

impl<Rng: crate::LoaderRng> BatchGeneratorImpl<Rng> {
    pub(super) fn new(seed: u64, batch_size: usize, context: TasksContext) -> Self {
        Self {
            seed,
            batch_size,
            context,
            _phantom: PhantomData,
        }
    }
}

impl<Rng: crate::LoaderRng> BatchGenerator for BatchGeneratorImpl<Rng> {
    type Task = Task;

    fn batch_size(&self) -> usize {
        self.batch_size
    }

    fn seed(&self) -> u64 {
        self.seed
    }

    fn generate(&mut self) -> Vec<Self::Task> {
        let mut batch = Vec::with_capacity(self.batch_size);
        let mut batch_rng = Rng::seed_from_u64(self.seed);
        while batch.len() != batch.capacity() {
            let task = match batch_rng.gen_range(0u8..1) {
                0 => upload_program_task::<Rng>(&mut self.context, batch_rng.next_u64()),
                1..=u8::MAX => unreachable!("Num of generators exhausted."),
            };
            batch.push(task);
        }
        batch
    }
}
