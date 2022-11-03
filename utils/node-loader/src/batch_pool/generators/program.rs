use crate::{batch_pool::Seed, utils::LoaderRng};
use arbitrary::Unstructured;

pub fn generate_gear_program<Rng: LoaderRng>(seed: Seed) -> Vec<u8> {
    let mut rng = Rng::seed_from_u64(seed);

    let mut buf = vec![0; 100_000];
    rng.fill_bytes(&mut buf);

    let mut u = Unstructured::new(&buf);

    let mut config = gear_wasm_gen::GearConfig::new_normal();
    config.print_test_info = Some(format!("Gear program seed = '{seed}'"));

    gear_wasm_gen::gen_gear_program_code(&mut u, config)
}
