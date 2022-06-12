#![no_main]

use economic_checks::*;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|params: ComposerParams| {
    economic_checks::run_target(&Params::Composer(params), composer_target);
});
