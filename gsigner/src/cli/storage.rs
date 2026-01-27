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
        scheme::{KeyGenerationResult, KeyInfo, SchemeFormatter},
        util::resolve_storage_location,
    },
    traits::{SeedableKey, SignatureScheme},
};
use anyhow::Result;
use secrecy::ExposeSecret;

#[cfg(all(feature = "keyring", feature = "serde"))]
pub trait StorageScheme: SignatureScheme + crate::keyring::KeyringScheme {}
#[cfg(all(feature = "keyring", feature = "serde"))]
impl<T> StorageScheme for T where T: SignatureScheme + crate::keyring::KeyringScheme {}

#[cfg(not(all(feature = "keyring", feature = "serde")))]
pub trait StorageScheme: SignatureScheme {}
#[cfg(not(all(feature = "keyring", feature = "serde")))]
impl<T> StorageScheme for T where T: SignatureScheme {}

pub fn create_signer<S>(storage: &StorageLocationArgs) -> Result<crate::Signer<S>>
where
    S: StorageScheme,
    S::PrivateKey: SeedableKey,
{
    let password = storage
        .storage_password
        .as_ref()
        .map(|p: &secrecy::SecretString| p.expose_secret().to_owned());
    if let Some(path) = resolve_storage_location(storage) {
        Ok(crate::Signer::fs_with_password(path, password)?)
    } else {
        Ok(crate::Signer::memory_with_password(password))
    }
}

pub fn with_signer<S, F, R>(storage: &StorageLocationArgs, f: F) -> Result<R>
where
    S: StorageScheme,
    S::PrivateKey: SeedableKey,
    F: FnOnce(crate::Signer<S>) -> Result<R>,
{
    f(create_signer::<S>(storage)?)
}

pub fn clear_keys_command<S>(
    storage: &StorageLocationArgs,
) -> Result<crate::cli::scheme::ClearResult>
where
    S: StorageScheme,
    S::PrivateKey: SeedableKey,
{
    let signer: crate::Signer<S> = create_signer(storage)?;
    let len = signer.list_keys()?.len();
    signer.clear_keys()?;
    Ok(crate::cli::scheme::ClearResult { removed: len })
}

pub fn generate_key_result<S>(
    storage: &StorageLocationArgs,
    formatter: &SchemeFormatter<S>,
    show_secret: bool,
) -> Result<KeyGenerationResult>
where
    S: StorageScheme,
    S::PrivateKey: SeedableKey + Clone,
{
    with_signer::<S, _, _>(storage, |signer| {
        let (private_key, public_key) = {
            let (pk, _) = S::generate_keypair();
            let public = signer.import_key(pk.clone())?;
            (pk, public)
        };

        let public_display = formatter.format_public(&public_key);
        let address = signer.address(public_key);
        let address_display = formatter.format_address(&address);

        Ok(KeyGenerationResult {
            public_key: public_display,
            address: address_display,
            scheme: formatter.scheme_name().to_string(),
            key_type: formatter.key_type(),
            secret: show_secret.then(|| hex::encode(private_key.seed().as_ref())),
            name: None,
        })
    })
}

pub fn key_info_from_public<S>(
    signer: &crate::Signer<S>,
    formatter: &SchemeFormatter<S>,
    public_key: S::PublicKey,
    show_secret: bool,
) -> Result<KeyInfo>
where
    S: StorageScheme,
    S::PrivateKey: SeedableKey,
{
    if !signer.has_key(public_key.clone())? {
        anyhow::bail!("Key not found in storage");
    }

    let address = signer.address(public_key.clone());
    let secret = if show_secret {
        let private_key = signer.get_private_key(public_key.clone())?;
        Some(hex::encode(private_key.seed().as_ref()))
    } else {
        None
    };

    Ok(KeyInfo {
        public_key: formatter.format_public(&public_key),
        address: formatter.format_address(&address),
        scheme: formatter.scheme_name().to_string(),
        key_type: formatter.key_type(),
        secret,
        name: None,
    })
}

#[cfg(any(feature = "ed25519", feature = "sr25519"))]
pub fn show_key_for_public<S>(
    storage: &StorageLocationArgs,
    formatter: &SchemeFormatter<S>,
    public_key: S::PublicKey,
    show_secret: bool,
) -> Result<crate::cli::scheme::ListKeysResult>
where
    S: StorageScheme,
    S::PrivateKey: SeedableKey,
{
    with_signer::<S, _, _>(storage, |signer| {
        let info = key_info_from_public(&signer, formatter, public_key, show_secret)?;
        Ok(crate::cli::scheme::ListKeysResult { keys: vec![info] })
    })
}

pub fn seed_from_hex<S>(hex_str: &str) -> Result<<S::PrivateKey as SeedableKey>::Seed>
where
    S: StorageScheme,
    S::PrivateKey: SeedableKey,
{
    let bytes = crate::cli::util::decode_hex_array::<32>(hex_str, "seed")?;
    let mut seed = <S::PrivateKey as SeedableKey>::Seed::default();
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
