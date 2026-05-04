// This file is part of Gear.
//
// Copyright (C) 2021-2026 Gear Technologies Inc.
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

//! Storage helpers shared by CLI handlers.

use crate::{
    cli::{
        commands::StorageLocationArgs,
        scheme::{KeyGenerationResult, KeyInfo},
        util::resolve_storage_location,
    },
    scheme::CryptoScheme,
};
use anyhow::Result;
use secrecy::ExposeSecret;

#[cfg(all(feature = "keyring", feature = "serde"))]
pub trait StorageScheme: CryptoScheme + crate::keyring::KeyringScheme {}
#[cfg(all(feature = "keyring", feature = "serde"))]
impl<T> StorageScheme for T where T: CryptoScheme + crate::keyring::KeyringScheme {}

#[cfg(not(all(feature = "keyring", feature = "serde")))]
pub trait StorageScheme: CryptoScheme {}
#[cfg(not(all(feature = "keyring", feature = "serde")))]
impl<T> StorageScheme for T where T: CryptoScheme {}

pub fn create_signer<S>(storage: &StorageLocationArgs) -> Result<crate::Signer<S>>
where
    S: StorageScheme,
{
    if let Some(path) = resolve_storage_location(storage) {
        let path = crate::keyring::resolve_namespaced_path(path, S::namespace());
        Ok(crate::Signer::fs(path)?)
    } else {
        Ok(crate::Signer::memory())
    }
}

pub fn with_signer<S, F, R>(storage: &StorageLocationArgs, f: F) -> Result<R>
where
    S: StorageScheme,
    F: FnOnce(crate::Signer<S>) -> Result<R>,
{
    f(create_signer::<S>(storage)?)
}

pub fn clear_keys_command<S>(
    storage: &StorageLocationArgs,
) -> Result<crate::cli::scheme::ClearResult>
where
    S: StorageScheme,
{
    let signer: crate::Signer<S> = create_signer(storage)?;
    let len = signer.list_keys()?.len();
    signer.clear_keys()?;
    Ok(crate::cli::scheme::ClearResult { removed: len })
}

pub fn generate_key_result<S>(
    storage: &StorageLocationArgs,
    show_secret: bool,
) -> Result<KeyGenerationResult>
where
    S: StorageScheme,
{
    with_signer::<S, _, _>(storage, |signer| {
        let password = storage
            .key_password
            .as_ref()
            .map(|p: &secrecy::SecretString| p.expose_secret().as_str());
        let (private_key, public_key) = {
            let (pk, _) = S::generate_keypair();
            let public = if let Some(pwd) = password {
                signer.import_encrypted(pk.clone(), pwd)?
            } else {
                signer.import(pk.clone())?
            };
            (pk, public)
        };

        let address = signer.address(public_key.clone());

        Ok(KeyGenerationResult {
            public_key: S::public_key_to_hex(&public_key),
            address: S::address_to_string(&address),
            scheme: S::NAME.to_string(),
            secret: show_secret.then(|| hex::encode(S::private_key_to_seed(&private_key).as_ref())),
            name: None,
        })
    })
}

pub fn key_info_from_public<S>(
    signer: &crate::Signer<S>,
    public_key: S::PublicKey,
    show_secret: bool,
    password: Option<&str>,
) -> Result<KeyInfo>
where
    S: StorageScheme,
{
    if !signer.has_key(public_key.clone())? {
        anyhow::bail!("Key not found in storage");
    }

    let address = signer.address(public_key.clone());
    let secret = if show_secret {
        let private_key = if let Some(pwd) = password {
            signer.private_key_encrypted(public_key.clone(), pwd)?
        } else {
            signer.private_key(public_key.clone())?
        };
        Some(hex::encode(S::private_key_to_seed(&private_key).as_ref()))
    } else {
        None
    };

    Ok(KeyInfo {
        public_key: S::public_key_to_hex(&public_key),
        address: S::address_to_string(&address),
        scheme: S::NAME.to_string(),
        secret,
        name: None,
    })
}

#[cfg(any(feature = "ed25519", feature = "sr25519"))]
pub fn show_key_for_public<S>(
    storage: &StorageLocationArgs,
    public_key: S::PublicKey,
    show_secret: bool,
    password: Option<&str>,
) -> Result<crate::cli::scheme::ListKeysResult>
where
    S: StorageScheme,
{
    with_signer::<S, _, _>(storage, |signer| {
        let info = key_info_from_public(&signer, public_key, show_secret, password)?;
        Ok(crate::cli::scheme::ListKeysResult { keys: vec![info] })
    })
}

pub fn seed_from_hex<S>(hex_str: &str) -> Result<S::Seed>
where
    S: StorageScheme,
{
    let bytes = crate::cli::util::decode_hex_array::<32>(hex_str, "seed")?;
    let mut seed = S::Seed::default();
    let buffer = seed.as_mut();
    if buffer.len() != bytes.len() {
        anyhow::bail!(
            "Invalid seed length: expected {} bytes, got {}",
            buffer.len(),
            bytes.len()
        );
    }
    buffer.copy_from_slice(&bytes);
    Ok(seed)
}
