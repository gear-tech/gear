// This file is part of Gear.
//
// Copyright (C) 2026 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use anyhow::{Context, Result};
use gear_node_wrapper::{Node, NodeInstance};
use regex::Regex;
use snapbox::cmd::{self, Command, OutputAssert};
use std::{
    env,
    path::{Path, PathBuf},
    sync::{Arc, LazyLock, Once, OnceLock},
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
        .unwrap_or_else(|| {
            PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("../target")
        })
        .join("release/gear")
}
