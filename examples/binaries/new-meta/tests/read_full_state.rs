use demo_new_meta::Wallet;
use gstd::Encode;
use gtest::System;

mod common;

#[test]
fn read_state_returns_full_state() {
    let system = System::new();
    let program = common::initialize_current_program(&system);

    let actual_state = program.read_state().expect("Unable to read program state");

    let expected_state = Wallet::test_sequence().encode();

    assert_eq!(actual_state, expected_state);
}
