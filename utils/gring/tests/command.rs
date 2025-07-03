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
#![cfg(feature = "cli")]

use anyhow::{anyhow, Result};
use gring::{cmd::Command, Keystore};
use std::{env, process};

fn gring(args: &[&str]) -> Result<String> {
    let path =
        env::var_os("NEXTEST_BIN_EXE_gring").unwrap_or_else(|| env!("CARGO_BIN_EXE_gring").into());

    let output = process::Command::new(path).args(args).output()?;
    if output.stdout.is_empty() {
        return Err(anyhow!(
            "stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let stdout = output.stdout;
    Ok(String::from_utf8_lossy(&stdout).to_string())
}

#[test]
fn new() -> Result<()> {
    let key = "_gring_test_new";
    let passphrase = "test";
    Command::New {
        name: key.to_string(),
        passphrase: passphrase.to_string(),
        vanity: None,
    }
    .run()?;

    let json = Command::store()?.join(format!("{key}.json"));
    assert!(json.exists());

    let keystore = serde_json::from_slice::<Keystore>(&std::fs::read(json)?)?;
    assert!(keystore.decrypt_scrypt(passphrase.as_bytes()).is_ok());
    Ok(())
}

#[test]
fn sign_and_verify() -> Result<()> {
    let key = "_gring_sig";
    let key2 = "_gring_sig_2";
    let message = "vara";

    gring(&["new", key, "-p", "test"])?;
    gring(&["use", key])?;
    let sign = gring(&["sign", message, "-p", "test"])?;
    let signature = sign
        .lines()
        .find(|line| line.contains("Signature"))
        .ok_or_else(|| anyhow!("Signature not found in output: {}", sign))?
        .split("Signature:")
        .collect::<Vec<&str>>()[1]
        .trim();
    assert!(gring(&["verify", message, signature])?.contains("Verified"));

    // `key2` can not verify this signature bcz it is signed by `key`.
    gring(&["new", key2, "-p", "test"])?;
    gring(&["use", key2])?;
    assert!(gring(&["verify", message, signature])?.contains("Not Verified"));
    Ok(())
}
