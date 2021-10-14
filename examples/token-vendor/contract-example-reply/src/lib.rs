#![no_std]

use gstd::{exec, msg, prelude::*, ProgramId};

gstd::metadata! {
    title: "GEAR Workshop Contract Example",
    init:
        input: Vec<u8>,
        output: Vec<u8>,
    handle:
        input: Vec<u8>,
        output: Vec<u8>
}
struct State {
    owner_id: Option<ProgramId>,
}

impl State {
    fn set_owner_id(&mut self, owner_id: Option<ProgramId>) {
        self.owner_id = owner_id;
    }
    fn owner_id(&self) -> Option<ProgramId> {
        self.owner_id
    }
}

static mut STATE: State = State { owner_id: None };

#[no_mangle]
pub unsafe extern "C" fn handle() {
    let mut hex_id = [0u8; 64];
    hex::encode_to_slice(
        STATE.owner_id().unwrap_or(msg::source()).as_slice(),
        &mut hex_id,
    );

    msg::reply(hex_id, exec::gas_available() / 2, 0);
}

#[no_mangle]
pub unsafe extern "C" fn init() {
    let msg = String::from_utf8(msg::load_bytes()).expect("Invalid message: should be utf-8");

    // set owner from payload or fallback to msg::source()
    let id = hex::decode(msg)
        .map(|bytes| ProgramId::from_slice(&bytes))
        .unwrap_or_else(|_| msg::source());

    // Set owner id from init payload
    STATE.set_owner_id(Some(id));

    msg::reply(b"INIT", exec::gas_available() / 2, 0);
}
