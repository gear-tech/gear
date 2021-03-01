use std::env;

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
            "join" => println!(
                "{:?}",
                RoomMessage::Join {
                    under_name: args[3].to_string()
                }
                .encode()
            ),
            "yell" => println!(
                "{:?}",
                RoomMessage::Yell {
                    text: args[3].to_string()
                }
                .encode()
            ),
            _ => (),
        },
        "member" => match args[2].as_str() {
            "private" => println!("{:?}", MemberMessage::Private(args[3].to_string()).encode()),
            "room" => println!("{:?}", MemberMessage::Room(args[3].to_string()).encode()),
            _ => (),
        },
        _ => (),
    }
}
