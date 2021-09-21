#![no_std]
#![feature(const_btree_new)]

extern crate alloc;

// for panic/oom handlers
extern crate gstd;

use alloc::{collections::BTreeSet, sync::Arc};
use core::cell::RefCell;
use core::num::ParseIntError;
use gstd::{ext, msg, prelude::*, ProgramId};
use gstd_async::msg as msg_async;
use gstd_meta::meta;
use spin;

meta! {
    title: "GEAR Token Vendor",
    // Any hex ProgramId
    input: Vec<u8>,
    output: Vec<u8>,
    // Hex Program Ids coma separated
    init_input: Vec<u8>,
    init_output: Vec<u8>
}

#[derive(Debug, Clone)]
struct State {
    owner_id: Option<ProgramId>,
    members: BTreeSet<ProgramId>,
    reward: u128,
}

impl State {
    fn new(owner_id: Option<ProgramId>, members: BTreeSet<ProgramId>, reward: u128) -> Self {
        Self {
            owner_id,
            members,
            reward,
        }
    }
    fn set_owner_id(&mut self, owner_id: Option<ProgramId>) {
        self.owner_id = owner_id;
    }
    fn owner_id(&mut self) -> Option<ProgramId> {
        self.owner_id
    }
    fn members(&self) -> &BTreeSet<ProgramId> {
        &self.members
    }
    fn insert_member(&mut self, id: &ProgramId) {
        self.members.remove(id);
    }
    fn remove_member(&mut self, id: &ProgramId) {
        self.members.remove(id);
    }
    fn exists(&self, id: &ProgramId) -> bool {
        self.members.get(id).is_some()
    }
}

static STATE: spin::Mutex<RefCell<State>> = spin::Mutex::new(RefCell::new(State {
    owner_id: None,
    members: BTreeSet::new(),
    reward: 10,
}));

#[gstd_async::main]
async fn main() {
    let msg = String::from_utf8(msg::load_bytes()).expect("Invalid message: should be utf-8");
    let id =
        ProgramId::from_slice(&decode_hex(&msg).expect("DECODE HEX FAILED: INVALID PROGRAM ID"));

    let state = STATE.lock();
    ext::debug(&format!("members: {:?}", state.borrow().members()));

    // If msg::source is registered in workshop then we send a message to id from payload
    if state.borrow().exists(&msg::source()) {
        drop(state);
        let reply =
            msg_async::send_and_wait_for_reply(id, b"verify", msg::gas_available() / 2, 0).await;

        let reply = String::from_utf8(reply).expect("Invalid message: should be utf-8");

        let member_id = ProgramId::from_slice(
            &decode_hex(&reply).expect("DECODE HEX FAILED: INVALID PROGRAM ID"),
        );
        if msg::source() == member_id {
            ext::debug(&format!(
                "SUCCESS:\nmember: {:?}\ncontract: {:?}",
                id,
                msg::source()
            ));
            let state = STATE.lock();

            state.borrow_mut().remove_member(&member_id);
            msg::send_with_value(member_id, b"success", 0, state.borrow().reward);
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn init() {
    let owner_id = msg::source();

    // "{id},{id},{id}" etc.
    let members_str =
        String::from_utf8(msg::load_bytes()).expect("Invalid message: should be utf-8");

    let members = members_str
        .split(',')
        .map(|member| {
            let member_id = ProgramId::from_slice(
                &decode_hex(member).expect("DECODE HEX FAILED: INVALID PROGRAM ID"),
            );
            member_id
        })
        .collect();

    STATE
        .lock()
        .replace(State::new(Some(owner_id), members, 10));

    msg::reply(b"INIT", 0, 0);
}

fn decode_hex(s: &str) -> Result<Vec<u8>, ParseIntError> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16))
        .collect()
}
