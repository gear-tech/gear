use demo_new_meta::{
    MessageInitIn, Person, Wallet, META_EXPORTS_V1, META_EXPORTS_V2, META_WASM_V1, META_WASM_V2,
};
use gstd::Encode;
use gtest::{state_args, Program, System};

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
    const FUNC_NAME: &str = "first_wallet";
    assert!(META_EXPORTS_V1.contains(&FUNC_NAME));

    let actual_state = program
        .read_state_bytes_using_wasm(FUNC_NAME, META_WASM_V1.to_vec(), state_args!())
        .expect("Unable to read program state");

    let expected_state = Wallet::test_sequence().first().encode();

    assert_eq!(expected_state, actual_state);
}

#[test]
fn read_state_bytes_with_parameterized_wasm_func_returns_transformed_state() {
    let system = System::new();
    let program = initialize_current_program(&system);
    const FUNC_NAME: &str = "wallet_by_person";
    assert!(META_EXPORTS_V2.contains(&FUNC_NAME));
    let other_person = Person {
        surname: "OtherSurname".into(),
        name: "OtherName".into(),
    };

    let actual_state = program
        .read_state_bytes_using_wasm(
            FUNC_NAME,
            META_WASM_V2.to_vec(),
            state_args!(other_person.clone()),
        )
        .expect("Unable to read program state");

    let expected_state = Wallet::test_sequence()
        .into_iter()
        .find(|wallet| wallet.person == other_person)
        .encode();

    assert_eq!(expected_state, actual_state);
}

#[test]
fn read_state_bytes_with_two_args_wasm_func_returns_transformed_state() {
    let system = System::new();
    let program = initialize_current_program(&system);
    const FUNC_NAME: &str = "wallet_by_name_and_surname";
    assert!(META_EXPORTS_V2.contains(&FUNC_NAME));

    let name = "OtherName".to_string();
    let surname = "OtherSurname".to_string();

    let actual_state = program
        .read_state_bytes_using_wasm(
            FUNC_NAME,
            META_WASM_V2.to_vec(),
            state_args!(name.clone(), surname.clone()),
        )
        .expect("Unable to read program state");

    let expected_state = Wallet::test_sequence()
        .into_iter()
        .find(|wallet| wallet.person.name == name && wallet.person.surname == surname)
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

#[test]
fn read_state_with_wasm_func_returns_transformed_state() {
    let system = System::new();
    let program = initialize_current_program(&system);
    const FUNC_NAME: &str = "first_wallet";
    assert!(META_EXPORTS_V1.contains(&FUNC_NAME));

    let actual_state = program
        .read_state_using_wasm(FUNC_NAME, META_WASM_V1.to_vec(), state_args!())
        .expect("Unable to read program state");

    let expected_state = Wallet::test_sequence().first().cloned();

    assert_eq!(expected_state, actual_state);
}

#[test]
fn read_state_with_parameterized_wasm_func_returns_transformed_state() {
    let system = System::new();
    let program = initialize_current_program(&system);
    const FUNC_NAME: &str = "wallet_by_person";
    assert!(META_EXPORTS_V2.contains(&FUNC_NAME));
    let other_person = Person {
        surname: "OtherSurname".into(),
        name: "OtherName".into(),
    };

    let actual_state = program
        .read_state_using_wasm(
            FUNC_NAME,
            META_WASM_V2.to_vec(),
            state_args!(other_person.clone()),
        )
        .expect("Unable to read program state");

    let expected_state = Wallet::test_sequence()
        .into_iter()
        .find(|wallet| wallet.person == other_person);

    assert_eq!(expected_state, actual_state);
}

#[test]
fn read_state_with_two_args_wasm_func_returns_transformed_state() {
    let system = System::new();
    let program = initialize_current_program(&system);
    const FUNC_NAME: &str = "wallet_by_name_and_surname";
    assert!(META_EXPORTS_V2.contains(&FUNC_NAME));

    let name = "OtherName".to_string();
    let surname = "OtherSurname".to_string();

    let actual_state = program
        .read_state_using_wasm(
            FUNC_NAME,
            META_WASM_V2.to_vec(),
            state_args!(name.clone(), surname.clone()),
        )
        .expect("Unable to read program state");

    let expected_state = Wallet::test_sequence()
        .into_iter()
        .find(|wallet| wallet.person.name == name && wallet.person.surname == surname);

    assert_eq!(expected_state, actual_state);
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
