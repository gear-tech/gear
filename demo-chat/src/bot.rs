use gstd::{ext, msg};

mod shared;

use shared::{RoomMessage, MemberMessage};
use codec::{Decode as _, Encode as _};

static mut NAME: String = String::new();
pub fn name() -> &'static str {
    unsafe { &NAME }
}

#[no_mangle]
pub unsafe extern "C" fn handle() {
    bot(MemberMessage::decode(&mut &msg::load()[..]).expect("Failed to decode incoming message"));
}

fn bot(message: MemberMessage) {
    use shared::MemberMessage::*;
    match message {
        Private(text) => {
            ext::debug(&format!("BOT '{}': received private message from #{}: '{}'", name(), msg::source(), text));
        },
        Room(text) => {
            ext::debug(&format!("BOT '{}': received room message from #{}: '{}'", name(), msg::source(), text));
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
            NAME = name.to_string();
            let room_id = room_id.parse::<u64>().expect("INTIALIZATION FAILED: INVALID ROOM ID");
            send_room(room_id, RoomMessage::Join{ under_name: name.to_string() });
        }
        _ => {
            ext::debug(&format!("INITLAIZATION FAILED"));
        }
    }

    ext::debug(&format!("BOT '{}' created", NAME));
}

fn main() {
}
