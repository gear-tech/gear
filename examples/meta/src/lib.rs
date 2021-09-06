#![no_std]

use gstd::prelude::*;
use gstd_meta::{meta, TypeInfo};

// Metatypes for input and output
#[derive(TypeInfo)]
pub struct MessageInitIn {
    pub currency: String,
    pub amount: u8,
}

#[derive(TypeInfo)]
pub struct MessageInitOut {
    pub rate: Result<u8, u8>,
    pub sum: u8,
}

#[derive(TypeInfo)]
pub struct MessageIn {
    pub id: Id,
}

#[derive(TypeInfo)]
pub struct MessageOut {
    pub res: Vec<Result<Wallet, String>>,
}

// Additional to primary types
#[derive(TypeInfo)]
pub struct Id {
    pub decimal: u64,
    pub hex: Vec<u8>,
}

#[derive(TypeInfo)]
pub struct Person {
    pub surname: String,
    pub name: String,
    pub patronymic: Option<String>,
}

#[derive(TypeInfo)]
pub struct Wallet {
    pub id: Id,
    pub person: Person,
}

meta! {
    title: "Example program with metadata",
    input: MessageIn,
    output: MessageOut,
    init_input: MessageInitIn,
    init_output: MessageInitOut,
    extra: Wallet, Id, Person
}

#[no_mangle]
pub unsafe extern "C" fn handle() {}

#[no_mangle]
pub unsafe extern "C" fn init() {}
