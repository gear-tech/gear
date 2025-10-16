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

//! Helpers for working with the local gsigner keyring store.

use anyhow::{Result, anyhow};
use gsigner::schemes::sr25519::Keyring;
use std::{fs, path::PathBuf};
use tracing::info;

const NEW_STORE_DIR: &str = "gsigner";
const LEGACY_STORE_DIR: &str = "gring";

/// Resolve the on-disk keyring store location, creating it when needed.
///
/// If a legacy `gring` directory exists and the new `gsigner` directory
/// has not been created yet, the legacy path is reused to preserve
/// existing keys.
pub fn store_path() -> Result<PathBuf> {
    let data_dir = dirs::data_dir().ok_or_else(|| anyhow!("Failed to locate app directory."))?;
    let new_store = data_dir.join(NEW_STORE_DIR);
    let legacy_store = data_dir.join(LEGACY_STORE_DIR);

    if new_store.exists() || !legacy_store.exists() {
        fs::create_dir_all(&new_store)?;
        info!("keyring store: {}", new_store.display());
        return Ok(new_store);
    }

    fs::create_dir_all(&legacy_store)?;
    info!("keyring store (legacy): {}", legacy_store.display());
    Ok(legacy_store)
}

/// Load the sr25519 keyring from disk.
pub fn load_keyring() -> Result<Keyring> {
    Keyring::load(store_path()?)
}
