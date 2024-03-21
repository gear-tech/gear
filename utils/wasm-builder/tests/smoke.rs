// This file is part of Gear.

// Copyright (C) 2022-2024 Gear Technologies Inc.
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
use std::{fs, process::Command, sync::OnceLock};

struct CargoRunner(Command);

impl CargoRunner {
    fn new() -> Self {
        Self(Command::new("cargo"))
    }

    fn stable() -> Self {
        let mut cmd = Command::new("cargo");
        cmd.arg("+stable");

        Self(cmd)
    }

    fn args<const SIZE: usize>(mut self, args: [&str; SIZE]) -> Self {
        self.0.args(args);
        self
    }

    #[track_caller]
    fn run(self) {
        let mut cmd = self.0;
        cmd.arg("--color=always");
        cmd.arg("--manifest-path=test-program/Cargo.toml");

        let status = cmd.status().expect("cargo run error");
        assert!(status.success());
    }
}

fn install_stable_toolchain() {
    static STABLE_TOOLCHAIN: OnceLock<()> = OnceLock::new();

    STABLE_TOOLCHAIN.get_or_init(|| {
        let status = Command::new("rustup")
            .arg("toolchain")
            .arg("install")
            .arg("stable")
            .arg("--component")
            .arg("llvm-tools")
            .arg("--target")
            .arg("wasm32-unknown-unknown")
            .status()
            .expect("rustup run error");
        assert!(status.success());
    });
}

#[ignore]
#[test]
fn test_debug() {
    install_stable_toolchain();

    CargoRunner::new().args(["test"]).run();
    CargoRunner::stable().args(["test"]).run();
}

#[ignore]
#[test]
fn build_debug() {
    install_stable_toolchain();

    CargoRunner::new().args(["build"]).run();
    CargoRunner::stable().args(["build"]).run();
}

#[ignore]
#[test]
fn test_release() {
    install_stable_toolchain();

    CargoRunner::new().args(["test", "--release"]).run();
    CargoRunner::stable().args(["test", "--release"]).run();
}

#[ignore]
#[test]
fn build_release() {
    install_stable_toolchain();

    CargoRunner::new().args(["build", "--release"]).run();
    CargoRunner::stable().args(["build", "--release"]).run();
}

#[test]
fn build_release_for_target() {
    CargoRunner::new()
        .args(["build", "--release", "--target", TARGET])
        .run();
}

#[ignore]
#[test]
fn no_infinite_build() {
    fs::write("test-program/src/rebuild_test.rs", "mod a {}").unwrap();

    CargoRunner::new().args(["build"]).run();

    fs::write("test-program/src/rebuild_test.rs", "mod b {}").unwrap();

    CargoRunner::new().args(["build"]).run();
}

#[ignore]
#[test]
fn features_tracking() {
    #[track_caller]
    fn read_export_entry(name: &str) -> Option<parity_wasm::elements::ExportEntry> {
        parity_wasm::deserialize_file(format!(
            "test-program/target/wasm32-unknown-unknown/{}/test_program.wasm",
            if cfg!(debug_assertions) {
                "debug"
            } else {
                "release"
            }
        ))
        .unwrap()
        .export_section()
        .unwrap()
        .entries()
        .iter()
        .find(|entry| entry.field() == name)
        .cloned()
    }

    CargoRunner::new().args(["build", "--features=a"]).run();
    assert!(read_export_entry("handle_reply").is_some());
    assert!(read_export_entry("handle_signal").is_none());
    CargoRunner::new().args(["build", "--features=b"]).run();
    assert!(read_export_entry("handle_signal").is_some());
    assert!(read_export_entry("handle_reply").is_none());
}
