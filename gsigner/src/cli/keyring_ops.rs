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

//! Shared keyring helpers and trait abstraction.

use crate::cli::{
    commands::StorageLocationArgs,
    scheme::{KeyGenerationResult, KeyInfo, SchemeResult},
    util::storage_root,
};
use anyhow::Result;
pub const MAX_VANITY_ATTEMPTS: usize = 100_000;
use secrecy::ExposeSecret;
use std::path::PathBuf;

#[cfg(feature = "keyring")]
pub(crate) enum KeyringLocation {
    Disk(PathBuf),
    Memory,
}

#[cfg(feature = "keyring")]
impl KeyringLocation {
    fn display(&self) -> String {
        match self {
            KeyringLocation::Disk(path) => path.display().to_string(),
            KeyringLocation::Memory => "<memory>".to_string(),
        }
    }
}

#[cfg(feature = "keyring")]
pub struct KeyringEntry {
    pub public_key: String,
    pub address: String,
    pub key_type: String,
    pub name: String,
    pub secret: Option<String>,
}

#[cfg(feature = "keyring")]
pub trait KeyringOps {
    type Keyring;

    fn namespace() -> &'static str;
    fn load(path: PathBuf) -> Result<Self::Keyring>;
    fn memory() -> Self::Keyring;
    fn create(
        keyring: &mut Self::Keyring,
        name: &str,
        password: Option<&str>,
    ) -> Result<KeyringEntry>;
    fn add_hex(
        keyring: &mut Self::Keyring,
        name: &str,
        hex: &str,
        password: Option<&str>,
    ) -> Result<KeyringEntry>;
    fn import_suri(
        keyring: &mut Self::Keyring,
        name: &str,
        suri: &str,
        suri_password: Option<&str>,
        encryption_password: Option<&str>,
    ) -> Result<KeyringEntry>;
    fn list(keyring: &Self::Keyring) -> Result<Vec<KeyringEntry>>;
    fn vanity(
        keyring: &mut Self::Keyring,
        name: &str,
        prefix: &str,
        password: Option<&str>,
    ) -> Result<KeyringEntry>;
}

#[cfg(feature = "keyring")]
fn resolve_keyring_location(
    args: &StorageLocationArgs,
    namespace: &'static str,
) -> Result<KeyringLocation> {
    if args.memory {
        Ok(KeyringLocation::Memory)
    } else {
        let path = crate::keyring::resolve_namespaced_path(storage_root(&args.path), namespace);
        Ok(KeyringLocation::Disk(path))
    }
}

#[cfg(feature = "keyring")]
pub(crate) fn with_keyring_instance<K, LoadFn, MemFn, F, R>(
    storage: StorageLocationArgs,
    namespace: &'static str,
    load_fn: LoadFn,
    memory_fn: MemFn,
    f: F,
) -> Result<R>
where
    LoadFn: Fn(PathBuf) -> Result<K>,
    MemFn: Fn() -> K,
    F: FnOnce(KeyringLocation, &mut K, Option<String>) -> Result<R>,
{
    let password = storage
        .storage_password
        .as_ref()
        .map(|p: &secrecy::SecretString| p.expose_secret().to_owned());
    let location = resolve_keyring_location(&storage, namespace)?;
    let mut keyring = match &location {
        KeyringLocation::Disk(path) => load_fn(path.clone())?,
        KeyringLocation::Memory => memory_fn(),
    };
    f(location, &mut keyring, password)
}

#[cfg(feature = "keyring")]
pub fn keygen_from_entry<S: crate::traits::SignatureScheme>(
    entry: KeyringEntry,
) -> KeyGenerationResult {
    KeyGenerationResult {
        public_key: entry.public_key,
        address: entry.address,
        scheme: S::scheme_name().to_string(),
        key_type: entry.key_type,
        secret: entry.secret,
        name: Some(entry.name),
    }
}

#[cfg(feature = "keyring")]
pub fn keyinfo_from_entry<S: crate::traits::SignatureScheme>(entry: KeyringEntry) -> KeyInfo {
    KeyInfo {
        public_key: entry.public_key,
        address: entry.address,
        scheme: S::scheme_name().to_string(),
        key_type: entry.key_type,
        secret: entry.secret,
        name: Some(entry.name),
    }
}

#[cfg(feature = "keyring")]
pub fn keyring_init<Ops: KeyringOps>(storage: StorageLocationArgs) -> Result<SchemeResult> {
    with_keyring_instance(
        storage,
        Ops::namespace(),
        Ops::load,
        Ops::memory,
        |location, _, _| {
            Ok(SchemeResult::Message(crate::cli::scheme::MessageResult {
                message: format!("Initialised keyring at {}", location.display()),
            }))
        },
    )
}

#[cfg(feature = "keyring")]
pub fn keyring_generate<Ops, S>(
    storage: StorageLocationArgs,
    name: String,
    show_secret: bool,
) -> Result<SchemeResult>
where
    Ops: KeyringOps,
    S: crate::traits::SignatureScheme,
{
    with_keyring_instance(
        storage,
        Ops::namespace(),
        Ops::load,
        Ops::memory,
        |_location, keyring, password| {
            let entry = Ops::create(keyring, &name, password.as_deref())?;
            let mut entry = entry;
            if !show_secret {
                entry.secret = None;
            }
            Ok(SchemeResult::Generate(keygen_from_entry::<S>(entry)))
        },
    )
}

#[cfg(feature = "keyring")]
pub fn keyring_import<Ops, S, F>(
    storage: StorageLocationArgs,
    _name: String,
    show_secret: bool,
    as_import: bool,
    f: F,
) -> Result<SchemeResult>
where
    Ops: KeyringOps,
    S: crate::traits::SignatureScheme,
    F: FnOnce(&mut Ops::Keyring, Option<&str>) -> Result<KeyringEntry>,
{
    with_keyring_instance(
        storage,
        Ops::namespace(),
        Ops::load,
        Ops::memory,
        |_location, keyring, password| {
            let entry = f(keyring, password.as_deref())?;
            let mut entry = entry;
            if !show_secret {
                entry.secret = None;
            }
            let keygen = keygen_from_entry::<S>(entry);
            if as_import {
                Ok(SchemeResult::Import(crate::cli::scheme::KeyImportResult {
                    public_key: keygen.public_key,
                    address: keygen.address,
                    scheme: keygen.scheme,
                    key_type: keygen.key_type,
                    secret: keygen.secret,
                    name: keygen.name,
                }))
            } else {
                Ok(SchemeResult::Generate(keygen))
            }
        },
    )
}

#[cfg(feature = "keyring")]
pub fn keyring_list<Ops, S>(storage: StorageLocationArgs) -> Result<SchemeResult>
where
    Ops: KeyringOps,
    S: crate::traits::SignatureScheme,
{
    with_keyring_instance(
        storage,
        Ops::namespace(),
        Ops::load,
        Ops::memory,
        |_location, keyring, _| {
            let keys: Vec<KeyInfo> = Ops::list(keyring)?
                .into_iter()
                .map(keyinfo_from_entry::<S>)
                .collect();
            Ok(SchemeResult::List(crate::cli::scheme::ListKeysResult {
                keys,
            }))
        },
    )
}

#[cfg(feature = "keyring")]
pub fn keyring_vanity<Ops, S>(
    storage: StorageLocationArgs,
    name: String,
    prefix: String,
    show_secret: bool,
) -> Result<SchemeResult>
where
    Ops: KeyringOps,
    S: crate::traits::SignatureScheme,
{
    with_keyring_instance(
        storage,
        Ops::namespace(),
        Ops::load,
        Ops::memory,
        |_location, keyring, password| {
            let entry = Ops::vanity(keyring, &name, &prefix, password.as_deref())?;
            let mut entry = entry;
            if !show_secret {
                entry.secret = None;
            }
            Ok(SchemeResult::Generate(keygen_from_entry::<S>(entry)))
        },
    )
}
