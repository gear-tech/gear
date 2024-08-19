// This file is part of Gear.

// Copyright (C) 2021-2024 Gear Technologies Inc.
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

//! Crate verifier

use crate::{handler, Simulator};
use anyhow::{anyhow, Result};
use serde::Deserialize;
use std::process::Command;

#[derive(Debug, Deserialize)]
struct Resp {
    pub versions: Vec<Version>,
}

#[derive(Debug, Deserialize)]
struct Version {
    pub num: String,
}

/// Verify if the package has already been published.
pub fn verify(name: &str, version: &str, simulator: Option<&Simulator>) -> Result<bool> {
    println!("Verifying {name}@{version} ...");

    let client = reqwest::blocking::Client::builder()
        .user_agent("gear-crates-io-manager")
        .build()?;

    if let Some(simulator) = simulator {
        if client
            .get(format!(
                "http://{}/api/v1/crates/{}/{version}/download",
                simulator.addr(),
                handler::crates_io_name(name)
            ))
            .send()?
            .error_for_status()
            .is_ok()
        {
            return Ok(true);
        }
    } else if let Ok(resp) = client
        .get(format!(
            "https://crates.io/api/v1/crates/{}/versions",
            handler::crates_io_name(name)
        ))
        .send()?
        .json::<Resp>()
    {
        return Ok(resp.versions.into_iter().any(|v| v.num == version));
    }

    Ok(false)
}

/// Get the short hash of the current commit.
pub fn hash() -> Result<String> {
    Ok(Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .map_err(|e| anyhow!("failed to execute command git, {e}"))?
        .stdout
        .iter()
        .filter_map(|&c| (!c.is_ascii_whitespace()).then_some(c as char))
        .collect())
}
