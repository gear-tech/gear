use gstd::prelude::*;
use gstd::*;

#[allow(dead_code)]
#[derive(TypeInfo)]
struct Carrot {
    fresh: bool,
    size: u8,
}

#[allow(dead_code)]
#[derive(TypeInfo)]
struct Bread {
    roasted: bool,
    width: u8,
}

#[allow(dead_code)]
#[derive(TypeInfo)]
struct Sandwich {
    bread: Bread,
    price: Option<u64>,
}

#[allow(dead_code)]
#[derive(TypeInfo)]
struct Salad {
    vegetables: Vec<Carrot>,
    finished: Result<u64, u8>,
}

meta! {
    title: "Example program with metadata",
    input: Bread,
    output: Sandwich,
    init_input: Salad,
    init_output: Salad,
    extra_types: Carrot
}

#[test]
fn title() {
    assert_eq!(unsafe { meta_title() }, "Example program with metadata",);
}

#[test]
fn input() {
    assert_eq!(
        unsafe { meta_input() },
        r#"{"Bread":{"roasted":"bool","width":"u8"},"Carrot":{"fresh":"bool","size":"u8"}}"#,
    );
}

#[test]
fn output() {
    assert_eq!(
        unsafe { meta_output() },
        r#"{"Carrot":{"fresh":"bool","size":"u8"},"Sandwich":{"bread":"Bread","price":"Option<u64>"}}"#,
    );
}

#[test]
fn init_input() {
    assert_eq!(
        unsafe { meta_init_input() },
        r#"{"Carrot":{"fresh":"bool","size":"u8"},"Salad":{"finished":"Result<u64, u8>","vegetables":"Vec<Carrot>"}}"#,
    );
}

#[test]
fn init_output() {
    assert_eq!(
        unsafe { meta_init_output() },
        r#"{"Carrot":{"fresh":"bool","size":"u8"},"Salad":{"finished":"Result<u64, u8>","vegetables":"Vec<Carrot>"}}"#,
    );
}
