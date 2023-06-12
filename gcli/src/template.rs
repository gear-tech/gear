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

//! Gear program template

use crate::result::Result;
use anyhow::anyhow;
use reqwest::Client;
use std::process::Command;

/// see https://docs.github.com/en/rest/repos/repos
const GEAR_DAPPS_GH_API: &str = "https://api.github.com/orgs/gear-dapps/repos";
const GEAR_DAPP_ORG: &str = "https://github.com/gear-dapps/";

/// Repo object from github api response.
#[derive(serde::Deserialize)]
struct Repo {
    pub name: String,
}

/// List all examples.
pub async fn list() -> Result<Vec<String>> {
    let r = Client::builder()
        .user_agent("gcli")
        .build()
        .map_err(|e| anyhow!("Failed to build http client: {}", e))?
        .get(GEAR_DAPPS_GH_API)
        .send()
        .await
        .map_err(|e| anyhow!("Failed to get examples: {}", e))?;

    let repos = r
        .json::<Vec<Repo>>()
        .await
        .map_err(|e| anyhow!("Failed to deserialize example list: {}", e))?
        .into_iter()
        .map(|repo| repo.name)
        .collect();

    Ok(repos)
}

/// Download example
pub async fn download(example: &str) -> Result<()> {
    let url = format!("{}{}.git", GEAR_DAPP_ORG, example);
    Command::new("git")
        .args(["clone", &url, "--depth=1"])
        .output()
        .map_err(|e| anyhow!("Failed to download example: {}", e))?;

    Ok(())
}

#[tokio::test]
async fn list_examples() {
    assert!(list()
        .await
        .expect("Failed to get examples")
        .contains(&"react-app".to_string()));
}
