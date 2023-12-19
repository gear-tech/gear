// This file is part of Gear.
//
// Copyright (C) 2021-2023 Gear Technologies Inc.
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

use crate::common::{
    self, env,
    node::{Convert, NodeExec},
    Args,
};
use anyhow::{anyhow, Result};
use std::{
    path::PathBuf,
    process::{Command, Stdio},
};

fn demo_messager() -> Result<PathBuf> {
    let path = PathBuf::from(env::bin("demo_messager"));

    if !path.exists()
        && !Command::new("cargo")
            .args([
                "build",
                "-p",
                "demo-messager",
                &format!("--{}", *env::PROFILE),
                "--features",
                "gcli",
            ])
            .status()?
            .success()
    {
        return Err(anyhow!("Failed to build demo-messager with feature gcli"));
    }

    Ok(path)
}

#[test]
fn embedded_gcli() -> Result<()> {
    let node = common::dev()?;
    let demo = Command::new(demo_messager()?)
        .args(Vec::<String>::from(Args::new("upload").endpoint(node.ws())))
        .stderr(Stdio::piped())
        .output()?;

    let stderr = demo.stderr.convert();
    assert!(
        !stderr.contains("Submitted Gear::upload_program"),
        "code should be uploaded, but got: {stderr}"
    );

    Ok(())
}
