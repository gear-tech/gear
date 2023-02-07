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

use gear_wasm_builder::TARGET;
use std::{fs, path::PathBuf, process::Command};

struct CargoRunner(Command);

impl CargoRunner {
    fn new() -> Self {
        Self(Command::new("cargo"))
    }

    fn args<const SIZE: usize>(mut self, args: [&str; SIZE]) -> Self {
        self.0.args(args);
        self
    }

    fn env(mut self, k: &str, v: &str) -> Self {
        self.0.env(k, v);
        self
    }

    fn run(self) -> bool {
        let mut cmd = self.0;
        cmd.arg("--color=always");
        cmd.arg("--manifest-path=test-program/Cargo.toml");

        let status = cmd.status().expect("cargo run error");
        status.success()
    }
}

#[test]
fn test_debug() {
    assert!(CargoRunner::new().args(["test"]).run());
}

#[test]
fn build_debug() {
    assert!(CargoRunner::new().args(["build"]).run());
}

#[test]
fn test_release() {
    assert!(CargoRunner::new().args(["test", "--release"]).run());
}

#[test]
fn build_release() {
    assert!(CargoRunner::new().args(["build", "--release"]).run());
}

#[test]
fn build_release_for_target() {
    assert!(CargoRunner::new()
        .args(["build", "--release", "--target", TARGET])
        .run());
}

#[test]
fn skip_wasm_build() {
    fn wasm_binary_rs() -> String {
        let out_dir = fs::read_to_string("test-program/.out_dir").unwrap();
        let out_dir = PathBuf::from(out_dir);
        let wasm_binary_rs = out_dir.join("wasm_binary.rs");
        fs::read_to_string(wasm_binary_rs).unwrap()
    }

    assert!(CargoRunner::new()
        .args(["build"])
        .env("SKIP_WASM_BUILD", "1")
        .run());

    assert!(wasm_binary_rs().contains("WASM_BINARY: &[u8] = &[]"));

    assert!(CargoRunner::new()
        .args(["build"])
        .env("SKIP_WASM_BUILD", "0")
        .run());

    assert!(wasm_binary_rs().contains("WASM_BINARY: &[u8] = include_bytes"));
}
