use demo_new_meta::{
    MessageInitIn, Person, Wallet, META_EXPORTS_V1, META_EXPORTS_V2, META_WASM_V1, META_WASM_V2,
};
use gstd::Encode;
use gtest::{Program, System};

#[test]
fn read_state_bytes_returns_full_state() {
    let system = System::new();
    let program = initialize_current_program(&system);

    let actual_state = program
        .read_state_bytes()
        .expect("Unable to read program state");

    let expected_state = Wallet::test_sequence().encode();

    assert_eq!(actual_state, expected_state);
}

#[test]
fn read_state_bytes_with_wasm_func_returns_transformed_state() {
    let system = System::new();
    let program = initialize_current_program(&system);
    const FIRST_WALLET_FUNC_NAME: &str = "first_wallet";
    assert!(META_EXPORTS_V1.contains(&FIRST_WALLET_FUNC_NAME));

    let actual_state = program
        .read_state_bytes_using_wasm(FIRST_WALLET_FUNC_NAME, META_WASM_V1.to_vec(), None)
        .expect("Unable to read program state");

    let expected_state = Wallet::test_sequence().first().encode();

    assert_eq!(expected_state, actual_state);
}

#[test]
fn read_state_bytes_with_parameterized_wasm_func_returns_transformed_state() {
    let system = System::new();
    let program = initialize_current_program(&system);
    const WALLET_BY_PERSON_FUNC_NAME: &str = "wallet_by_person";
    assert!(META_EXPORTS_V2.contains(&WALLET_BY_PERSON_FUNC_NAME));
    let other_person = Person {
        surname: "OtherSurname".into(),
        name: "OtherName".into(),
    };

    let actual_state = program
        .read_state_bytes_using_wasm(
            WALLET_BY_PERSON_FUNC_NAME,
            META_WASM_V2.to_vec(),
            Some(other_person.encode()),
        )
        .expect("Unable to read program state");

    let expected_state = Wallet::test_sequence()
        .into_iter()
        .find(|wallet| wallet.person == other_person)
        .encode();

    assert_eq!(expected_state, actual_state);
}

#[test]
fn read_state_returns_full_state() {
    let system = System::new();
    let program = initialize_current_program(&system);

    let actual_state: Vec<Wallet> = program.read_state().expect("Unable to read program state");

    let expected_state = Wallet::test_sequence();

    assert_eq!(actual_state, expected_state);
}

fn initialize_current_program(system: &System) -> Program {
    const SOME_USER_ID: u64 = 3;
    let program = Program::current(system);
    program.send(
        SOME_USER_ID,
        MessageInitIn {
            amount: 123,
            currency: "USD".into(),
        },
    );
    program
}
