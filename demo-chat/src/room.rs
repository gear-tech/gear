use gstd::{ext, msg};

mod shared;

use shared::{RoomMessage, MemberMessage};
use codec::{Decode as _, Encode as _};

static mut ROOM_NAME: String = String::new();
pub fn room_name() -> &'static str {
    unsafe { &ROOM_NAME as _ }
}
static mut MEMBERS: Vec<(u64, String)> = Vec::new();
pub fn add_member(id: u64, name: String) {
    unsafe {
        MEMBERS.push((id, name))
    }
}
pub fn members() -> impl Iterator<Item=&'static (u64, String)> {
    unsafe { MEMBERS.iter() }
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
        },
        Yell { text } => {
            for (id, _) in members() {
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
    ROOM_NAME = String::from_utf8(msg::load()).expect("Invalid message: should be utf-8");
    ext::debug(&format!("ROOM '{}' created", ROOM_NAME));
}

fn main() {
}
