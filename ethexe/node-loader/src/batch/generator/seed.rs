use crate::args::SeedVariant;
use gear_call_gen::{CallGenRng, CallGenRngCore};
use rand::RngCore;

pub fn some_generator<Rng: CallGenRng>(code_seed_type: SeedVariant) -> Box<dyn CallGenRngCore> {
    match code_seed_type {
        SeedVariant::Dynamic(v) => Box::new(Rng::seed_from_u64(v)) as _,
        SeedVariant::Constant(v) => Box::new(ConstantGenerator::new(v)) as _,
    }
}

#[derive(Debug, Clone, Copy)]
struct ConstantGenerator(u64);

impl ConstantGenerator {
    fn new(v: u64) -> Self {
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

    fn fill_bytes(&mut self, dest: &mut [u8]) {
        let mut i = 0;
        while i < dest.len() {
            let bytes = self.next_u64().to_le_bytes();
            let take = (dest.len() - i).min(bytes.len());
            dest[i..i + take].copy_from_slice(&bytes[..take]);
            i += take;
        }
    }

    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), rand::Error> {
        self.fill_bytes(dest);
        Ok(())
    }
}
