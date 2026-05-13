// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use gear_node_wrapper::Node;
use std::{env, path::PathBuf, thread, time::Duration};

#[ignore]
#[test]
fn run() {
    let node =
        PathBuf::from(env::var_os("GEAR_WORKSPACE_DIR").unwrap()).join("target/release/gear");
    let node = Node::from_path(node).unwrap().spawn().unwrap();

    loop {
        thread::sleep(Duration::from_secs(3));
        println!("logs: {:#?}", node.logs());
    }
}
