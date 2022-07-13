#![cfg(feature = "builder")]

use gear_program::builder::Pre;

#[test]
fn check_spec_version() {
    if let Err((runtime, api)) = Pre::default().check_spec_version() {
        panic!("Gear api outdated, expected spec_version: {runtime:?}, actual: {api:?}")
    }
}
