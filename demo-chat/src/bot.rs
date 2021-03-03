use gstd::{ext, msg};
use std::ptr;

mod shared;

use codec::{Decode as _, Encode as _};
use shared::{MemberMessage, RoomMessage};

#[derive(Debug)]
struct State {
    pub name: &'static str,
}

impl State {
    fn set_name(&mut self, name: &'static str) {
        self.name = &name;
    }
}

static mut _STATE: ptr::NonNull<State> = ptr::NonNull::<State>::dangling();

impl Drop for State {
    fn drop(&mut self) {
        ext::debug(&format!("Dropped state"));
    }
}

pub fn name() -> &'static str {
    unsafe {
        let state = _STATE.as_mut();
        state.name
    }
}

#[no_mangle]
pub unsafe extern "C" fn handle() {
    bot(MemberMessage::decode(&mut &msg::load()[..]).expect("Failed to decode incoming message"));
}

fn bot(message: MemberMessage) {
    use shared::MemberMessage::*;
    match message {
        Private(text) => {
            ext::debug(&format!(
                "BOT '{}': received private message from #{}: '{}'",
                name(),
                msg::source(),
                text
            ));
        }
        Room(text) => {
            ext::debug(&format!(
                "BOT '{}': received room message from #{}: '{}'",
                name(),
                msg::source(),
                text
            ));
        }
    }
}

pub fn send_room(id: u64, msg: RoomMessage) {
    let mut encoded = vec![];
    msg.encode_to(&mut encoded);
    msg::send(id, &encoded);
}

#[no_mangle]
pub unsafe extern "C" fn init() {
    let input = String::from_utf8(msg::load()).expect("Invalid message: should be utf-8");

    let split = input.split(' ').collect::<Vec<_>>();
    match split.len() {
        2 => {
            let (name, room_id) = (&split[0], &split[1]);
            // let s: &'static str = Box::leak(*name);
            // let s: &'static str = Box::leak(*name);
            let state = _STATE.as_mut();
            let s: &'static str = Box::leak(name.to_string().into_boxed_str());
            state.set_name(s);
            let room_id = room_id
                .parse::<u64>()
                .expect("INTIALIZATION FAILED: INVALID ROOM ID");
            send_room(
                room_id,
                RoomMessage::Join {
                    under_name: name.to_string(),
                },
            );
        }
        _ => {
            ext::debug(&format!("INITLAIZATION FAILED"));
        }
    }

    ext::debug(&format!("BOT '{}' created", name()));
}

fn main() {}
