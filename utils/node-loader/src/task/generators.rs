use std::pin::Pin;

use arbitrary::Unstructured;
use futures::Future;
use rand::RngCore;

use crate::{
    args::SeedVariant,
    utils::{self, now},
};

pub(crate) fn get_some_seed_generator<Rng: utils::Rng>(
    seed_variant: Option<SeedVariant>,
) -> Box<dyn RngCore + Send + Sync> {
    match seed_variant {
        None => Box::new(Rng::seed_from_u64(now())) as Box<dyn RngCore + Send + Sync>,
        Some(SeedVariant::Dynamic(v)) => Box::new(Rng::seed_from_u64(v)) as Box<dyn RngCore + Send + Sync>,
        Some(SeedVariant::Constant(v)) => Box::new(ConstantGenerator::new(v)) as Box<dyn RngCore + Send + Sync>,
    }
}

pub(crate) fn generate_gear_program<Rng: utils::Rng>(seed: u64) -> Vec<u8> {
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

pub(crate) type FutureSomeReporter = Pin<Box<dyn Future<Output = crate::reporter::SomeReporter> + Send + 'static>>;

pub(crate) trait TaskGen{
    type Output;
    fn gen(self: &Self) -> Self::Output;
}
