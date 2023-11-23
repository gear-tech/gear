//! Crate verifier
#![allow(unused)]

use anyhow::Result;
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Deserialize)]
struct Resp {
    pub versions: Vec<Version>,
}

#[derive(Debug, Deserialize)]
struct Version {
    pub num: String,
}

/// Verify if the package has already been published.
pub fn verify(name: &str, version: &str) -> Result<bool> {
    let client = reqwest::blocking::Client::builder()
        .user_agent("gear-crates-io-manager")
        .build()?;

    let resp = client
        .get(&format!("https://crates.io/api/v1/crates/{name}/versions"))
        .send()?
        .json::<Resp>()?;

    Ok(resp.versions.into_iter().any(|v| v.num == version))
}
