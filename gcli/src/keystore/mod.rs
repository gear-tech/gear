// This file is part of Gear.
//
// Copyright (C) 2021-2022 Gear Technologies Inc.
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

//! keystore

pub mod json;
pub mod key;

use crate::{
    keystore::key::Key,
    result::{Error, Result},
    utils,
};
use gsdk::{config::GearConfig, ext::sp_core::sr25519, PairSigner};
use lazy_static::lazy_static;
use std::{
    fs,
    path::{Path, PathBuf},
};

lazy_static! {
    // @WARNING: THIS WILL ONLY BE SECURE IF THE keystore IS SECURE.
    // when you have NO PASSWORD, If it can be got by an attacker then
    // they can also get your key.
    static ref KEYSTORE_PATH: PathBuf = utils::home().join("keystore");

    // @WARNING: THIS WILL ONLY BE SECURE IF THE keystore IS SECURE.
    // when you have NO PASSWORD, If it can be got by an attacker then
    // they can also get your key.
    static ref KEYSTORE_JSON_PATH: PathBuf = utils::home().join("keystore.json");
}

/// Generate a new keypair.
///
/// @WARNING: THIS WILL ONLY BE SECURE IF THE keystore IS SECURE.
/// when you have NO PASSWORD, If it can be got by an attacker then
/// they can also get your key.
pub fn generate(passwd: Option<&str>) -> Result<PairSigner<GearConfig, sr25519::Pair>> {
    let pair = Key::generate_with_phrase::<sr25519::Pair>(passwd)?;
    fs::write(&*KEYSTORE_PATH, pair.1)?;

    Ok(pair.0)
}

/// Login with suri.
///
/// @WARNING: THIS WILL ONLY BE SECURE IF THE keystore IS SECURE.
/// when you have NO PASSWORD, If it can be got by an attacker then
/// they can also get your key.
pub fn login(suri: &str, passwd: Option<&str>) -> Result<PairSigner<GearConfig, sr25519::Pair>> {
    let pair = Key::from_string(suri).pair::<sr25519::Pair>(passwd)?;
    fs::write(&*KEYSTORE_PATH, suri)?;

    Ok(pair.0)
}

/// Get signer from cache.
///
/// @WARNING: THIS WILL ONLY BE SECURE IF THE keystore IS SECURE.
/// when you have NO PASSWORD, If it can be got by an attacker then
/// they can also get your key.
pub fn cache(passwd: Option<&str>) -> Result<PairSigner<GearConfig, sr25519::Pair>> {
    let pair = if (*KEYSTORE_PATH).exists() {
        let suri = fs::read_to_string(&*KEYSTORE_PATH).map_err(|_| Error::Logout)?;
        Key::from_string(&suri).pair::<sr25519::Pair>(passwd)?.0
    } else if (*KEYSTORE_JSON_PATH).exists() {
        decode_json_file(&*KEYSTORE_JSON_PATH, passwd)?
    } else {
        return Err(Error::Logout);
    };

    Ok(pair)
}

/// Get signer from keyring.
///
/// @WARNING: THIS WILL ONLY BE SECURE IF THE keyring IS SECURE.
/// when you have NO PASSWORD, If it can be got by an attacker then
/// they can also get your key.
pub fn keyring(passwd: Option<&str>) -> Result<PairSigner<GearConfig, sr25519::Pair>> {
    Ok(Key::from_keyring()?.pair::<sr25519::Pair>(passwd)?.0)
}

/// Decode pair from json file.
///
/// @WARNING: THIS WILL ONLY BE SECURE IF THE keystore IS SECURE.
/// when you have NO PASSWORD, If it can be got by an attacker then
/// they can also get your key.
pub fn decode_json_file(
    path: impl AsRef<Path>,
    passphrase: Option<&str>,
) -> Result<PairSigner<GearConfig, sr25519::Pair>> {
    let encrypted = serde_json::from_slice::<json::Encrypted>(&fs::read(&path)?)?;
    let pair = encrypted.create(passphrase.ok_or(Error::InvalidPassword)?)?;

    fs::copy(path, &*KEYSTORE_JSON_PATH)?;
    Ok(PairSigner::new(pair))
}
