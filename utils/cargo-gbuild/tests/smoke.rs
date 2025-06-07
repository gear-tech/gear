// This file is part of Gear.
//
// Copyright (C) 2024-2025 Gear Technologies Inc.
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
use cargo_gbuild::GBuild;
use gtest::{constants::DEFAULT_USER_ALICE, Program, System};
use std::{env, path::PathBuf, process::Command};

fn ping(sys: &System, prog: PathBuf) -> Program<'_> {
    // Get program from artifact
    let user = DEFAULT_USER_ALICE;
    let program = Program::from_file(sys, prog);

    // Init program
    let msg_id = program.send_bytes(user, b"PING");
    let res = sys.run_next_block();
    assert!(res.succeed.contains(&msg_id));
    assert!(res.contains(&(user, b"INIT_PONG")));

    // Handle program
    let msg_id = program.send_bytes(user, b"PING");
    let res = sys.run_next_block();
    assert!(res.succeed.contains(&msg_id));
    assert!(res.contains(&(user, b"HANDLE_PONG")));

    program
}

// NOTE:
//
// This test gathers both workspace build and single package build
// for avoiding asynchronous I/O of the built programs, use [`tokio::fs`]
// when this test grows.
#[test]
fn test_compile() -> Result<()> {
    let root = env::var("CARGO_MANIFEST_DIR").context("CARGO_MANIFEST_DIR not found")?;
    let root = PathBuf::from(root).join("test-program/Cargo.toml");
    let system = System::new();
    system.init_logger();

    // 1. Test single package build.
    let mut gbuild = GBuild::default().manifest_path(root);
    let artifacts = gbuild.build()?;
    ping(&system, artifacts.root.join("gbuild_test_program.wasm"));

    // 2. Test workspace build.
    gbuild = gbuild.workspace();
    let artifacts = gbuild.build()?;
    ping(&system, artifacts.root.join("gbuild_test_foo.wasm"));
    ping(&system, artifacts.root.join("gbuild_test_bar.wasm"));

    Ok(())
}

#[test]
fn test_program_tests() {
    // NOTE: workaround for installing stable toolchain if not exist
    // This is momently only for adapting the environment (nightly)
    // of our CI.
    {
        if option_env!("IN_NIX_SHELL").is_none() {
            let targets = Command::new("rustup")
                .args(["target", "list", "--toolchain", "stable"])
                .output()
                .expect("Failed to list rust toolchains")
                .stdout;

            if !String::from_utf8_lossy(&targets).contains("wasm32v1-none (installed)") {
                assert!(Command::new("rustup")
                    .args([
                        "toolchain",
                        "install",
                        "stable",
                        "--target",
                        "wasm32v1-none",
                    ])
                    .status()
                    .expect("Failed to install stable toolchain")
                    .success());
            }
        }
    }

    assert!(Command::new("cargo")
        .current_dir("test-program")
        .args(["+stable", "test", "--manifest-path", "Cargo.toml"])
        .status()
        .expect("Failed to run the tests of cargo-gbuild/test-program")
        .success());
}
