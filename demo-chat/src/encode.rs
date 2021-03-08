use std::env;
use std::fmt::Write;

mod shared;

use codec::{Decode as _, Encode as _};
use shared::{MemberMessage, RoomMessage};

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 4 {
        println!("Not enough arguments");
        std::process::exit(0);
    }
    match args[1].as_str() {
        "room" => match args[2].as_str() {
            "join" => {
                let out = RoomMessage::Join {
                    under_name: args[3].to_string(),
                }
                .encode();
                let mut s = String::from("0x");
                for byte in out {
                    write!(s, "{:02x}", byte);
                }
                println!("{:?}", s);
            }
            "yell" => {
                let out = RoomMessage::Yell {
                    text: args[3].to_string(),
                }
                .encode();
                let mut s = String::from("0x");
                for byte in out {
                    write!(s, "{:02x}", byte);
                }
                println!("{:?}", s);
            }
            _ => (),
        },
        "member" => match args[2].as_str() {
            "private" => {
                let out = MemberMessage::Private(args[3].to_string()).encode();
                let mut s = String::from("0x");
                for byte in out {
                    write!(s, "{:02x}", byte);
                }
                println!("{:?}", s);
            }
            "room" => {
                let out = MemberMessage::Room(args[3].to_string()).encode();
                let mut s = String::from("0x");
                for byte in out {
                    write!(s, "{:02x}", byte);
                }
                println!("{:?}", s);
            }
            _ => (),
        },
        _ => (),
    }
}
