#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|a: u64| {
    println!("generated - {a}");
    let _ = node_fuzzer::run(a);
});
