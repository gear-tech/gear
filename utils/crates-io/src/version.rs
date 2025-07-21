// This file is part of Gear.

// Copyright (C) 2021-2025 Gear Technologies Inc.
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

use crate::{EXPECTED_OWNERS, Simulator, handler};
use anyhow::{Result, anyhow};
use reqwest::{Client, StatusCode};
use serde::Deserialize;
use std::process::Command;

#[derive(Debug, Deserialize)]
struct VersionsResponse {
    versions: Vec<Version>,
}

#[derive(Debug, Deserialize)]
struct Version {
    num: String,
}

/// Verify if the package has already been published.
pub async fn verify(name: &str, version: &str, simulator: Option<&Simulator>) -> Result<bool> {
    println!("Verifying {name}@{version} ...");

    let client = Client::builder()
        .user_agent("gear-crates-io-manager")
        .build()?;

    if let Some(simulator) = simulator {
        if client
            .get(format!(
                "http://{}/api/v1/crates/{}/{version}/download",
                simulator.addr(),
                handler::crates_io_name(name)
            ))
            .send()
            .await?
            .error_for_status()
            .is_ok()
        {
            return Ok(true);
        }
    } else if let Ok(response) = client
        .get(format!(
            "https://crates.io/api/v1/crates/{}/versions",
            handler::crates_io_name(name)
        ))
        .send()
        .await?
        .json::<VersionsResponse>()
        .await
    {
        return Ok(response.versions.into_iter().any(|v| v.num == version));
    }

    Ok(false)
}

#[derive(Debug, Deserialize)]
struct OwnersResponse {
    users: Vec<User>,
}

#[derive(Debug, Deserialize)]
struct User {
    login: String,
}

/// Package status.
#[derive(Debug, PartialEq)]
pub enum PackageStatus {
    /// Package has not been published.
    NotPublished,
    /// Package has invalid owners.
    InvalidOwners,
    /// Package has valid owners.
    ValidOwners,
}

/// Verify if the package has valid owners.
pub async fn verify_owners(name: &str) -> Result<PackageStatus> {
    println!("Verifying {name} owners ...");

    let client = Client::builder()
        .user_agent("gear-crates-io-manager")
        .build()?;

    let response = client
        .get(format!(
            "https://crates.io/api/v1/crates/{}/owners",
            handler::crates_io_name(name)
        ))
        .send()
        .await?;

    if response.status() == StatusCode::NOT_FOUND {
        return Ok(PackageStatus::NotPublished);
    }

    let response = response.json::<OwnersResponse>().await?;
    let package_status = if response.users.len() == EXPECTED_OWNERS.len()
        && EXPECTED_OWNERS
            .into_iter()
            .all(|owner| response.users.iter().any(|u| u.login == owner))
    {
        PackageStatus::ValidOwners
    } else {
        PackageStatus::InvalidOwners
    };

    Ok(package_status)
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
