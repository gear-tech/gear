// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use gear_node_wrapper::{Node, NodeInstance};
use gsdk::{Api, SignedApi};
use std::{env, env::consts::EXE_EXTENSION, path::PathBuf};

pub async fn dev_node() -> (NodeInstance, SignedApi) {
    // Use release build because of performance reasons.
    let mut bin_path =
        PathBuf::from(env::var_os("GEAR_WORKSPACE_DIR").unwrap()).join("target/release/gear");
    bin_path.set_extension(EXE_EXTENSION);

    let node = Node::from_path(bin_path)
        .expect("Failed to start node: Maybe it isn't built with --release flag?")
        .spawn()
        .expect("Failed to spawn node process");

    let api = Api::new(&node.ws()).await.unwrap().signed_as_alice();

    (node, api)
}
