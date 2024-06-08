use gnode::Node;
use std::thread;

#[ignore]
#[test]
fn run() {
    let node = Node::from_path("../../target/release/gear")
        .unwrap()
        .spawn()
        .unwrap();

    loop {
        thread::sleep_ms(3000);
        println!("logs: {:#?}", node.logs);
    }
}
