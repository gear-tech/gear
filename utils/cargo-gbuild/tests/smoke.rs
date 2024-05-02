// This file is part of Gear.
//
// Copyright (C) 2024 Gear Technologies Inc.
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

use anyhow::Result;
use cargo_gbuild::GBuild;
use gtest::{Program, System};
use std::{path::PathBuf, process::Command};

#[test]
fn test_compile_program() -> Result<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test-program/Cargo.toml");
    let artifact = GBuild {
        manifest_path: root.to_string_lossy().to_string().into(),
        features: vec!["debug".into()],
        profile: None,
        target_dir: None,
        release: false,
    }
    .run()?;

    // Initialize system environment
    let system = System::new();
    system.init_logger();

    // Get program from artifact
    let user = 0;
    let program = Program::from_file(&system, artifact.program);

    // Init program
    let res = program.send_bytes(user, b"PING");
    assert!(!res.main_failed());
    assert!(res.contains(&(user, b"INIT_PONG")));

    // Handle program
    let res = program.send_bytes(user, b"PING");
    assert!(!res.main_failed());
    assert!(res.contains(&(user, b"HANDLE_PONG")));
    Ok(())
}

#[test]
fn test_program_tests() {
    assert!(Command::new("cargo")
        .current_dir("test-program")
        .args(["test"])
        .status()
        .expect("Failed to run the tests of cargo-gbuild/test-program")
        .success())
}
