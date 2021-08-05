#![no_std]
#![feature(default_alloc_error_handler)]
use gstd::{ext, msg, prelude::*, ProgramId};

use codec::{Decode as _, Encode as _};
use demo_chat::shared::{MemberMessage, RoomMessage};

#[derive(Debug)]
struct State {
    room_name: &'static str,
    members: Vec<(ProgramId, String)>,
}

impl State {
    fn set_room_name(&mut self, name: &'static str) {
        self.room_name = name;
    }
    fn add_member(&mut self, member: (ProgramId, String)) {
        self.members.push(member);
    }
    fn get_member(&self, id: ProgramId) -> Option<&(ProgramId, String)> {
        self.members.iter().find(|(member, _name)| *member == id)
    }
    fn room_name(&self) -> &'static str {
        ext::debug(&format!("room_name ptr -> {:p}", self.room_name));
        self.room_name
    }
}

static mut STATE: State = State {
    room_name: "",
    members: vec![],
};

pub fn send_member(id: ProgramId, msg: MemberMessage) {
    let mut encoded = vec![];
    msg.encode_to(&mut encoded);
    msg::send(id, &encoded, u64::MAX);
}

#[no_mangle]
pub unsafe extern "C" fn handle() {
    room(RoomMessage::decode(&mut &msg::load()[..]).expect("Failed to decode incoming message"));
}

unsafe fn room(room_msg: RoomMessage) {
    use RoomMessage::*;

    match room_msg {
        Join { under_name } => {
            ext::debug(&format!(
                "ROOM '{}': '{}' joined",
                STATE.room_name(),
                &under_name
            ));
            STATE.add_member((msg::source(), under_name));
        }
        Yell { text } => {
            ext::debug(&format!("Yell ptr -> {:p}", text.as_ptr()));
            for (id, _) in STATE.members.iter() {
                if *id != msg::source() {
                    send_member(
                        *id,
                        MemberMessage::Room(format!(
                            "{}: {}",
                            STATE
                                .get_member(msg::source())
                                .unwrap_or(&(ProgramId::default(), STATE.room_name().to_string()))
                                .1,
                            text
                        )),
                    )
                }
            }
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn init() {
    let s: &'static str = Box::leak(
        String::from_utf8(msg::load())
            .expect("Invalid message: should be utf-8")
            .into_boxed_str(),
    );
    STATE.set_room_name(s);
    ext::debug(&format!("ROOM '{}' created", STATE.room_name()));
}
