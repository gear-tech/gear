// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use anyhow::Result;
use gear_node_wrapper::{Node, NodeInstance};
use snapbox::cmd::{self, Command};
use std::{
    env,
    path::{Path, PathBuf},
};

pub fn init_node() -> Result<(NodeInstance, impl Fn() -> Command)> {
    let node = Node::from_path(node_bin())?.spawn()?;
    let node_ws = node.ws();

    let gcli = move || {
        Command::new(gcli_bin())
            .env(
                "GCLI_CONFIG_DIR",
                env::temp_dir().join("gcli-tests").join("config"),
            )
            .args(["--endpoint", &node_ws])
    };

    Ok((node, gcli))
}

fn gcli_bin() -> &'static Path {
    cmd::cargo_bin!("gcli")
}

fn node_bin() -> PathBuf {
    env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env::var_os("GEAR_WORKSPACE_DIR").unwrap()).join("target"))
        .join("release/gear")
}
