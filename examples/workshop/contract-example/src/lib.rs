#![no_std]

use gstd::{msg, prelude::*, ProgramId};
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
        msg::send(msg::source(), hex_id, msg::gas_available() / 2);
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
    let mut bytes = [0u8; 32];

    let id = match hex::decode_to_slice(&msg, &mut bytes) {
        Ok(()) => ProgramId::from_slice(&bytes),
        Err(_) => msg::source(),
    };

    // Set owner id from init payload
    STATE.set_owner_id(Some(id));

    msg::reply(b"INIT", 1000, 0);
}
