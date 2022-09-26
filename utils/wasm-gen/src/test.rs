use crate::{gen_gear_program_code, GearConfig};
use arbitrary::Unstructured;
use rand::{rngs::SmallRng, RngCore, SeedableRng};

#[test]
fn gen_wasm() {
    let mut rng = SmallRng::seed_from_u64(1234);
    for _ in 0..100 {
        let mut buf = vec![0; 1000000];
        rng.fill_bytes(&mut buf);
        let mut u = Unstructured::new(&buf);
        let code = gen_gear_program_code(&mut u, GearConfig::default());
        let _wat = wasmprinter::print_bytes(&code).unwrap();
    }
}

#[test]
fn gen_wasm_rare() {
    let mut rng = SmallRng::seed_from_u64(12345);
    for _ in 0..100 {
        let mut buf = vec![0; 1000000];
        rng.fill_bytes(&mut buf);
        let mut u = Unstructured::new(&buf);
        let code = gen_gear_program_code(&mut u, GearConfig::new_for_rare_cases());
        let _wat = wasmprinter::print_bytes(&code).unwrap();
    }
}
