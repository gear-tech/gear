// This file is part of Gear.

// Copyright (C) 2022 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::process::Command;

use gear_wasm_builder::TARGET;

fn run_cargo(args: &[&str]) -> bool {
    let mut cmd = Command::new("cargo");
    cmd.args(args);
    cmd.arg("--color=always");
    cmd.arg("--manifest-path=test-program/Cargo.toml");

    let status = cmd.status().expect("cargo run error");
    status.success()
}

#[test]
fn test_debug() {
    assert!(run_cargo(&["test"]));
}

#[test]
fn build_debug() {
    assert!(run_cargo(&["build"]));
}

#[test]
fn test_release() {
    assert!(run_cargo(&["test", "--release"]));
}

#[test]
fn build_release() {
    assert!(run_cargo(&["build", "--release"]));
}

#[test]
fn build_release_for_target() {
    assert!(run_cargo(&["build", "--release", "--target", TARGET]));
}
