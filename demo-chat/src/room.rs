use gstd::{ext, msg};
use std::ptr;
mod shared;

use codec::{Decode as _, Encode as _};
use shared::{MemberMessage, RoomMessage};

#[derive(Debug)]
struct State {
    pub room_name: &'static str,
    pub members: Vec<(u64, String)>,
}

impl State {
    fn set_room_name(&mut self, name: &'static str) {
        self.room_name = &name;
    }
    fn add_member(&mut self, member: (u64, String)) {
        self.members.push(member);
    }
}

static mut _STATE: ptr::NonNull<State> = ptr::NonNull::<State>::dangling();


impl Drop for State {
    fn drop(&mut self) {
        ext::debug(&format!("Dropped state"));
    }
}

pub fn room_name() -> &'static str {
    unsafe {
        let state = _STATE.as_mut();
        &state.room_name
    }
}

pub fn add_member(id: u64, name: String) {
    unsafe {
        let state = _STATE.as_mut();
        state.add_member((id, name));
    }
}

pub fn send_member(id: u64, msg: MemberMessage) {
    let mut encoded = vec![];
    msg.encode_to(&mut encoded);
    msg::send(id, &encoded);
}

#[no_mangle]
pub unsafe extern "C" fn handle() {
    room(RoomMessage::decode(&mut &msg::load()[..]).expect("Failed to decode incoming message"));
}

fn room(room_msg: RoomMessage) {
    use shared::RoomMessage::*;
    match room_msg {
        Join { under_name } => {
            ext::debug(&format!("ROOM '{}': '{}' joined", room_name(), &under_name));
            add_member(msg::source(), under_name);
        }
        Yell { text } => unsafe {
            let state = _STATE.as_mut();
            ext::debug(&format!("State - {:?}", state));

            for (id, _) in state.members.iter() {
                if *id != msg::source() {
                    send_member(
                        *id,
                        MemberMessage::Room(format!("#{}: {}", msg::source(), text)),
                    )
                }
            }
        },
    }
}

#[no_mangle]
pub unsafe extern "C" fn init() {
    let state = _STATE.as_mut();
    let s: &'static str = Box::leak(
        String::from_utf8(msg::load())
            .expect("Invalid message: should be utf-8")
            .into_boxed_str(),
    );
    state.set_room_name(s);
    ext::debug(&format!("ROOM '{}' created", room_name()));
}

fn main() {}
