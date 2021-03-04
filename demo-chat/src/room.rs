use gstd::{ext, msg};
mod shared;

use codec::{Decode as _, Encode as _};
use shared::{MemberMessage, RoomMessage};

#[derive(Debug)]
struct State {
    room_name: &'static str,
    members: Vec<(u64, String)>,
}

impl State {
    fn set_room_name(&mut self, name: &'static str) {
        self.room_name = &name;
    }
    fn add_member(&mut self, member: (u64, String)) {
        self.members.push(member);
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

pub fn send_member(id: u64, msg: MemberMessage) {
    let mut encoded = vec![];
    msg.encode_to(&mut encoded);
    msg::send(id, &encoded);
}

#[no_mangle]
pub unsafe extern "C" fn handle() {
    room(RoomMessage::decode(&mut &msg::load()[..]).expect("Failed to decode incoming message"));
}

unsafe fn room(room_msg: RoomMessage) {
    use shared::RoomMessage::*;

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
                        MemberMessage::Room(format!("#{}: {}", msg::source(), text)),
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

fn main() {}
