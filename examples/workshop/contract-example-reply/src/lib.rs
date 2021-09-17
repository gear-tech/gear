#![no_std]
#![feature(const_btree_new)]

extern crate alloc;

// for panic/oom handlers
extern crate gstd;

use core::num::ParseIntError;
use gstd::{ext, msg, prelude::*, ProgramId};
use gstd_meta::meta;

meta! {
    title: "GEAR Workshop Contract Example",
    input: Vec<u8>,
    output: Vec<u8>,
    // Owner id
    init_input: Vec<u8>,
    init_output: Vec<u8>
}

#[derive(Debug)]
struct State {
    owner_id: Option<ProgramId>,
}

impl State {
    fn set_owner_id(&mut self, owner_id: Option<ProgramId>) {
        self.owner_id = owner_id;
    }
    fn owner_id(&mut self) -> Option<ProgramId> {
        self.owner_id
    }
}

static mut STATE: State = State { owner_id: None };

#[no_mangle]
pub unsafe extern "C" fn handle() {
    if let Some(id) = STATE.owner_id() {
        let mut hex_id = [0u8; 64];
        hex::encode_to_slice(id.as_slice(), &mut hex_id);
        msg::reply(hex_id, msg::gas_available() / 2, 0);
    }
}

#[no_mangle]
pub unsafe extern "C" fn handle_reply() {
    // Send result to the owner of contract
    msg::send(
        STATE.owner_id().expect("owner id is set"),
        msg::load_bytes().as_slice(),
        0,
    );
}

#[no_mangle]
pub unsafe extern "C" fn init() {
    let msg = String::from_utf8(msg::load_bytes()).expect("Invalid message: should be utf-8");

    // set owner from payload or fallback to msg::source()
    let id = match decode_hex(&msg) {
        Ok(bytes) => ProgramId::from_slice(&bytes),
        Err(_) => msg::source(),
    };
    // Set owner id from init payload
    STATE.set_owner_id(Some(id));

    msg::reply(b"INIT", 0, 0);
}

fn decode_hex(s: &str) -> Result<Vec<u8>, ParseIntError> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16))
        .collect()
}
