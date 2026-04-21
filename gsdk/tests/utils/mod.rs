// This file is part of Gear.
//
// Copyright (C) 2023-2025 Gear Technologies Inc.
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

use gear_node_wrapper::{Node, NodeInstance};
use gsdk::{Api, SignedApi};
use std::{env, env::consts::EXE_EXTENSION, path::PathBuf, sync::Once};

fn probe_binary_once(bin_path: &std::path::Path) {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        eprintln!("[gsdk-test-diag] probing {bin_path:?}");
        eprintln!(
            "[gsdk-test-diag] exists={} is_file={}",
            bin_path.exists(),
            bin_path.is_file()
        );
        match std::process::Command::new(bin_path).arg("--version").output() {
            Ok(out) => {
                eprintln!("[gsdk-test-diag] --version exit status: {}", out.status);
                eprintln!(
                    "[gsdk-test-diag] --version stdout: {:?}",
                    String::from_utf8_lossy(&out.stdout)
                );
                eprintln!(
                    "[gsdk-test-diag] --version stderr: {:?}",
                    String::from_utf8_lossy(&out.stderr)
                );
            }
            Err(e) => {
                eprintln!("[gsdk-test-diag] --version spawn failed: {e}");
            }
        }
    });
}

pub async fn dev_node() -> (NodeInstance, SignedApi) {
    // Use release build because of performance reasons.
    let bin_path = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let mut bin_path = bin_path.join("../target/release/gear");
    bin_path.set_extension(EXE_EXTENSION);

    probe_binary_once(&bin_path);

    let node = Node::from_path(bin_path)
        .expect("Failed to start node: Maybe it isn't built with --release flag?")
        .spawn()
        .expect("Failed to spawn node process");

    let api = match Api::new(&node.ws()).await {
        Ok(api) => api.signed_as_alice(),
        Err(err) => {
            eprintln!("[gsdk-test-diag] Api::new({}) failed: {err:#}", node.ws());
            eprintln!("[gsdk-test-diag] Node logs:");
            match node.logs() {
                Ok(logs) if !logs.is_empty() => {
                    for line in logs {
                        eprintln!("[gsdk-test-diag]   {line}");
                    }
                }
                Ok(_) => eprintln!("[gsdk-test-diag]   <node produced no log lines>"),
                Err(e) => eprintln!("[gsdk-test-diag]   <failed to read node logs: {e}>"),
            }
            panic!("Api::new failed, see node logs above");
        }
    };

    (node, api)
}
