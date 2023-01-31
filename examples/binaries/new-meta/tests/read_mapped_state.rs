use demo_new_meta::{Person, Wallet, META_EXPORTS_V1, META_EXPORTS_V2, META_WASM_V1, META_WASM_V2};
use gstd::Encode;
use gtest::System;

mod common;

#[test]
fn read_state_with_map_returns_mapped_state() {
    let system = System::new();
    let program = common::initialize_current_program(&system);
    const FIRST_WALLET_FUNC_NAME: &str = "first_wallet";
    assert!(META_EXPORTS_V1.contains(&FIRST_WALLET_FUNC_NAME));

    let actual_state = program
        .read_state_with_map(META_WASM_V1.to_vec(), FIRST_WALLET_FUNC_NAME, None)
        .expect("Unable to read program state");

    let expected_state = Wallet::test_sequence().first().encode();

    assert_eq!(expected_state, actual_state);
}

#[test]
fn read_state_with_parameterized_map() {
    let system = System::new();
    let program = common::initialize_current_program(&system);
    const WALLET_BY_PERSON_FUNC_NAME: &str = "wallet_by_person";
    assert!(META_EXPORTS_V2.contains(&WALLET_BY_PERSON_FUNC_NAME));
    let other_person = Person {
        surname: "OtherSurname".into(),
        name: "OtherName".into(),
    };

    let actual_state = program
        .read_state_with_map(
            META_WASM_V2.to_vec(),
            WALLET_BY_PERSON_FUNC_NAME,
            Some(other_person.encode()),
        )
        .expect("Unable to read program state");

    let expected_state = Wallet::test_sequence()
        .into_iter()
        .find(|wallet| wallet.person == other_person)
        .encode();

    assert_eq!(expected_state, actual_state);
}
