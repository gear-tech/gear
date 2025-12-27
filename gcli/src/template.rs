// This file is part of Gear.
//
// Copyright (C) 2021-2025 Gear Technologies Inc.
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

use anyhow::{Context, Result};
use etc::{Etc, FileSystem, Read, Write};
use reqwest::Client;
use std::{env, path::Path, process::Command};

const GITHUB_TOKEN: &str = "GITHUB_TOKEN";

/// see https://docs.github.com/en/rest/repos/repos
const GEAR_DAPPS_GH_API: &str = "https://api.github.com/orgs/gear-foundation/repos";
const GEAR_DAPP_ORG: &str = "https://github.com/gear-foundation/";

/// Repo object from github api response.
#[derive(serde::Deserialize)]
struct Repo {
    pub name: String,
}

/// List all examples.
pub async fn list() -> Result<Vec<String>> {
    let mut rb = Client::builder()
        .user_agent("gcli")
        .build()
        .context("failed to build http client")?
        .get(GEAR_DAPPS_GH_API);

    if let Ok(tk) = env::var(GITHUB_TOKEN) {
        rb = rb.bearer_auth(tk);
    }

    let resp = rb.send().await.context("failed to get examples")?;

    let repos = resp
        .json::<Vec<Repo>>()
        .await
        .context("failed to deserialize example list")?
        .into_iter()
        .map(|repo| repo.name)
        .collect();

    Ok(repos)
}

/// Download example
pub async fn download(example: &str, path: &Path) -> Result<()> {
    let url = format!("{GEAR_DAPP_ORG}{example}.git");
    Command::new("git")
        .args(["clone", "--depth=1", &url])
        .arg(path)
        .status()
        .context("failed to download example")?;

    let repo = Etc::new(path)?;
    repo.rm(".git")?;

    // Init new git repo.
    Command::new("git")
        .arg("init")
        .arg(path)
        .status()
        .context("failed to init git")?;

    // Find all manifests
    let mut manifests = Vec::new();
    repo.find_all("Cargo.toml", &mut manifests)?;

    // Update each manifest
    for manifest in manifests {
        let manifest = Etc::new(manifest)?;
        let mut toml =
            String::from_utf8_lossy(&manifest.read().context("failed to read Cargo.toml")?)
                .to_string();

        process_manifest(&mut toml)?;
        manifest.write(toml.as_bytes())?;
    }

    Ok(())
}

/// Update project manifest.
fn process_manifest(manifest: &mut String) -> Result<()> {
    let (user, email) = {
        let user_bytes = Command::new("git")
            .args(["config", "--global", "--get", "user.name"])
            .output()?
            .stdout;
        let user = String::from_utf8_lossy(&user_bytes);

        let email_bytes = Command::new("git")
            .args(["config", "--global", "--get", "user.email"])
            .output()?
            .stdout;
        let email = String::from_utf8_lossy(&email_bytes);

        (user.to_string(), email.to_string())
    };

    *manifest = manifest.replace(
        r#"authors = ["Gear Technologies"]"#,
        &format!("authors = [\"{} <{}>\"]", user.trim(), email.trim()),
    );

    Ok(())
}

#[tokio::test]
async fn list_examples() {
    let ls = list().await.expect("Failed to get examples");
    // TODO: #2914
    assert!(
        ls.contains(&"dapp-template".to_string()),
        "all templates: {ls:?}"
    );
}
