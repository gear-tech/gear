use gnode::Node;
use std::{thread, time::Duration};

#[ignore]
#[test]
fn run() {
    let node = Node::from_path("../../target/release/gear")
        .unwrap()
        .spawn()
        .unwrap();

    loop {
        thread::sleep(Duration::from_secs(3));
        println!("logs: {:#?}", node.logs());
    }
}
