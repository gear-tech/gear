#![no_std]
#![feature(const_btree_new)]

extern crate alloc;

// for panic/oom handlers
extern crate gstd;

use alloc::collections::BTreeMap;
use core::num::ParseIntError;
use gstd::{ext, msg, prelude::*, ProgramId};
use gstd_meta::meta;

meta! {
    title: "GEAR Workshop",
    // Any hex ProgramId
    input: Vec<u8>,
    output: Vec<u8>,
    // Hex Program Ids coma separated
    init_input: Vec<u8>,
    init_output: Vec<u8>
}

#[derive(Debug)]
struct State {
    owner_id: Option<ProgramId>,
    members: BTreeMap<ProgramId, Option<ProgramId>>,
}

impl State {
    fn set_owner_id(&mut self, owner_id: Option<ProgramId>) {
        self.owner_id = owner_id;
    }
    fn owner_id(&mut self) -> Option<ProgramId> {
        self.owner_id
    }
    fn members(&mut self) -> &mut BTreeMap<ProgramId, Option<ProgramId>> {
        &mut self.members
    }
}

static mut STATE: State = State {
    owner_id: None,
    members: BTreeMap::new(),
};

static TOKEN_AMOUNT: u128 = 10;

#[no_mangle]
pub unsafe extern "C" fn handle() {
    let msg = String::from_utf8(msg::load_bytes()).expect("Invalid message: should be utf-8");
    let id =
        ProgramId::from_slice(&decode_hex(&msg).expect("DECODE HEX FAILED: INVALID PROGRAM ID"));

    ext::debug(&format!("msg = {}", msg));

    // If msg::source is registered in workshop then we send a message to id from payload
    if let Some(member) = STATE.members().get_mut(&msg::source()) {
        *member = Some(id);
        msg::send(id, b"verify", msg::gas_available() / 2);
    }

    // If contract send some id back
    if let Some((member, Some(contract))) = STATE.members().get_key_value(&id) {
        // Check answer from contract with member that submitted this contract id
        if contract == &msg::source() && member == &id {
            ext::debug(&format!(
                "SUCCESS:\nmember: {:?}\ncontract: {:?}",
                id,
                msg::source()
            ));
            STATE.members().remove(&id);
            msg::send_with_value(id, b"success", 0, TOKEN_AMOUNT);
        }
    }
}

// Contracts also can reply to the messages so we need to handle them too
#[no_mangle]
pub unsafe extern "C" fn handle_reply() {
    let msg = String::from_utf8(msg::load_bytes()).expect("Invalid message: should be utf-8");
    let id =
        ProgramId::from_slice(&decode_hex(&msg).expect("DECODE HEX FAILED: INVALID PROGRAM ID"));

    ext::debug(&format!("msg = {}", msg));
    ext::debug(&format!("src = {:?}", msg::source()));

    if let Some((member, Some(contract))) = STATE.members().get_key_value(&id) {
        if contract == &msg::source() && member == &id {
            ext::debug(&format!(
                "SUCCESS:\nmember: {:?}\ncontract: {:?}",
                id,
                msg::source()
            ));
            msg::send_with_value(*member, b"success", 0, TOKEN_AMOUNT);
            STATE.members().remove(member);
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn init() {
    let owner_id = msg::source();

    STATE.set_owner_id(Some(owner_id));

    // "{id},{id},{id}" etc.
    let members_str =
        String::from_utf8(msg::load_bytes()).expect("Invalid message: should be utf-8");

    members_str.split(',').for_each(|member| {
        let member_id = ProgramId::from_slice(
            &decode_hex(member).expect("DECODE HEX FAILED: INVALID PROGRAM ID"),
        );
        STATE.members().insert(member_id, None);
    });

    msg::reply(b"INIT", 0, 0);
}

fn decode_hex(s: &str) -> Result<Vec<u8>, ParseIntError> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16))
        .collect()
}
