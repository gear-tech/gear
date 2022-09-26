//! build script for gear-program cli
use std::{env, process::Command};

/// build gear-node
fn check_node() {
    println!("cargo:rerun-if-changed=../runtime");
    println!("cargo:rerun-if-changed=../pallets/gear");

    let mut cargo = Command::new("cargo");
    let profile = std::env::var("PROFILE").unwrap();
    let node = env!("CARGO_MANIFEST_DIR").to_owned() + "../node";

    cargo.args(&[
        "build",
        "--manifest-path",
        &node,
        &("--".to_owned() + &profile),
    ]);
}

fn main() {
    // 0. check if need to rebuild the node.
    check_node();
}
