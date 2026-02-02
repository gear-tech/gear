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

//! Shared keyring helpers and trait abstraction.

use crate::cli::{
    commands::{StorageLocationArgs, StorageLocationPathArgs},
    scheme::{KeyGenerationResult, KeyInfo, SchemeResult},
    util::storage_root,
};
use anyhow::Result;
pub const MAX_VANITY_ATTEMPTS: usize = 100_000;
use secrecy::ExposeSecret;
use std::path::PathBuf;

#[cfg(feature = "keyring")]
use crate::keyring::{KeyCodec, SubstrateKeystore};

/// Trait for scheme-specific secret display in CLI output.
///
/// Different schemes display their secrets differently:
/// - secp256k1: redacted display via `to_string()`
/// - ed25519/sr25519: hex-encoded bytes
#[cfg(feature = "keyring")]
pub trait PrivateKeyDisplay {
    type PrivateKey;

    /// Format the private key for CLI display.
    fn display_secret(private_key: &Self::PrivateKey) -> String;
}

/// Trait for scheme-specific vanity address matching.
///
/// Different schemes use different address formats:
/// - secp256k1: hex address prefix matching
/// - ed25519/sr25519: SS58 address prefix matching
#[cfg(feature = "keyring")]
pub trait VanityMatcher {
    type PrivateKey;
    type Address;

    /// Normalize user-provided prefix (remove 0x for hex, etc).
    fn normalize_prefix(prefix: &str) -> Result<String>;

    /// Check if a generated key's address matches the vanity prefix.
    fn matches_vanity(private_key: &Self::PrivateKey, prefix: &str) -> Result<bool>;
}

/// Trait for scheme-specific hex import handling.
///
/// Sr25519 has special handling for 32-byte seeds vs 96-byte keypairs.
#[cfg(feature = "keyring")]
pub trait HexImporter {
    type PrivateKey;

    /// Import private key from hex string.
    /// Default implementation uses KeyCodec::decode_private.
    fn import_hex(hex: &str) -> Result<Self::PrivateKey>;
}

/// Trait for scheme-specific public key extraction from keystore.
///
/// Sr25519 dynamically derives public key from private key on list.
#[cfg(feature = "keyring")]
pub trait PublicKeyExtractor<C: KeyCodec> {
    /// Extract public key hex from keystore entry.
    /// Default uses the stored public_key field.
    fn extract_public_key(keystore: &SubstrateKeystore<C>) -> String {
        keystore.public_key.clone()
    }
}

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
        .key_password
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
pub fn keygen_from_entry<S: crate::scheme::CryptoScheme>(
    entry: KeyringEntry,
) -> KeyGenerationResult {
    KeyGenerationResult {
        public_key: entry.public_key,
        address: entry.address,
        scheme: S::NAME.to_string(),
        secret: entry.secret,
        name: Some(entry.name),
    }
}

#[cfg(feature = "keyring")]
pub fn keyinfo_from_entry<S: crate::scheme::CryptoScheme>(entry: KeyringEntry) -> KeyInfo {
    KeyInfo {
        public_key: entry.public_key,
        address: entry.address,
        scheme: S::NAME.to_string(),
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
    S: crate::scheme::CryptoScheme,
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
    S: crate::scheme::CryptoScheme,
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
    S: crate::scheme::CryptoScheme,
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
    S: crate::scheme::CryptoScheme,
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

/// Trait for scheme-specific keyring command handling.
///
/// This trait allows schemes to provide custom implementations for commands
/// that have scheme-specific behavior (Sign, Show, Import) while sharing
/// the common infrastructure.
#[cfg(feature = "keyring")]
pub trait KeyringCommandHandler: KeyringOps {
    /// The scheme type for this handler.
    type Scheme: crate::scheme::CryptoScheme + crate::keyring::KeyringScheme;

    /// Handle the Sign command.
    fn handle_sign(
        storage: &StorageLocationArgs,
        public_key: &str,
        data: &str,
        prefix: &Option<String>,
        context: &Option<String>,
        contract: &Option<String>,
    ) -> Result<SchemeResult>;

    /// Handle the Show command.
    ///
    /// Default implementation parses public key from hex and uses `show_key_for_public`.
    /// Secp256k1 overrides this to also accept addresses.
    #[cfg(any(feature = "ed25519", feature = "sr25519"))]
    fn handle_show(
        storage: &StorageLocationArgs,
        key: &str,
        show_secret: bool,
    ) -> Result<SchemeResult> {
        use crate::scheme::CryptoScheme;
        let public = Self::Scheme::public_key_from_hex(key)?;
        let password = storage
            .key_password
            .as_ref()
            .map(|secret| secret.expose_secret().as_str());
        let result = crate::cli::storage::show_key_for_public::<Self::Scheme>(
            storage,
            public,
            show_secret,
            password,
        )?;
        Ok(SchemeResult::List(result))
    }

    /// Handle the Show command (required when only secp256k1 is enabled).
    #[cfg(not(any(feature = "ed25519", feature = "sr25519")))]
    fn handle_show(
        storage: &StorageLocationArgs,
        key: &str,
        show_secret: bool,
    ) -> Result<SchemeResult>;

    /// Handle the Import command.
    fn handle_import(
        storage: StorageLocationArgs,
        suri: Option<String>,
        seed: Option<String>,
        private_key: Option<String>,
        suri_password: Option<String>,
        name: Option<String>,
        show_secret: bool,
    ) -> Result<SchemeResult>;

    /// Handle the Clear command.
    fn handle_clear(storage: StorageLocationPathArgs) -> Result<SchemeResult> {
        let storage_args = storage.into_storage_args();
        let result = crate::cli::storage::clear_keys_command::<Self::Scheme>(&storage_args)?;
        Ok(SchemeResult::Clear(result))
    }
}

/// Execute a keyring command using a unified handler.
#[cfg(feature = "keyring")]
pub fn execute_keyring_command<H>(
    command: crate::cli::commands::SchemeKeyringCommands,
) -> Result<SchemeResult>
where
    H: KeyringCommandHandler,
{
    use crate::cli::commands::SchemeKeyringCommands;

    match command {
        SchemeKeyringCommands::Generate {
            storage,
            show_secret,
        } => keyring_generate::<H, H::Scheme>(storage, generate_key_name(), show_secret),
        SchemeKeyringCommands::Clear { storage } => H::handle_clear(storage),
        SchemeKeyringCommands::Sign {
            public_key,
            data,
            prefix,
            storage,
            context,
            contract,
        } => H::handle_sign(&storage, &public_key, &data, &prefix, &context, &contract),
        SchemeKeyringCommands::Show {
            storage,
            key,
            show_secret,
        } => H::handle_show(&storage, &key, show_secret),
        SchemeKeyringCommands::Vanity {
            storage,
            name,
            prefix,
            show_secret,
        } => keyring_vanity::<H, H::Scheme>(storage, name, prefix, show_secret),
        SchemeKeyringCommands::Init { storage } => keyring_init::<H>(storage),
        SchemeKeyringCommands::Create {
            storage,
            name,
            show_secret,
        } => keyring_generate::<H, H::Scheme>(storage, name, show_secret),
        SchemeKeyringCommands::Import {
            suri,
            seed,
            private_key,
            suri_password,
            name,
            storage,
            show_secret,
        } => H::handle_import(
            storage,
            suri,
            seed,
            private_key,
            suri_password,
            name,
            show_secret,
        ),
        SchemeKeyringCommands::List { storage } => {
            keyring_list::<H, H::Scheme>(storage.into_storage_args())
        }
    }
}

/// Generate a default key name based on timestamp.
#[cfg(feature = "keyring")]
fn generate_key_name() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    format!("key_{}", timestamp)
}

/// Generic `KeyringOps` implementation for schemes using `SubstrateKeystore<C>`.
///
/// This struct provides a unified implementation for all cryptographic schemes,
/// using the extension traits to handle scheme-specific behavior.
#[cfg(feature = "keyring")]
pub struct GenericKeyringOps<C, Ext> {
    _codec: std::marker::PhantomData<C>,
    _ext: std::marker::PhantomData<Ext>,
}

#[cfg(feature = "keyring")]
impl<C, Ext> Default for GenericKeyringOps<C, Ext> {
    fn default() -> Self {
        Self {
            _codec: std::marker::PhantomData,
            _ext: std::marker::PhantomData,
        }
    }
}

/// Combined extension trait for GenericKeyringOps.
#[cfg(feature = "keyring")]
pub trait KeyringOpsExt<C: KeyCodec>:
    PrivateKeyDisplay<PrivateKey = C::PrivateKey>
    + VanityMatcher<PrivateKey = C::PrivateKey>
    + HexImporter<PrivateKey = C::PrivateKey>
    + PublicKeyExtractor<C>
{
    /// The keyring namespace (e.g., "secp", "ed", "sr").
    const NAMESPACE: &'static str;
}

#[cfg(feature = "keyring")]
impl<C, Ext> KeyringOps for GenericKeyringOps<C, Ext>
where
    C: KeyCodec + 'static,
    Ext: KeyringOpsExt<C> + 'static,
{
    type Keyring = crate::keyring::Keyring<SubstrateKeystore<C>>;

    fn namespace() -> &'static str {
        Ext::NAMESPACE
    }

    fn load(path: PathBuf) -> Result<Self::Keyring> {
        Self::Keyring::load(path)
    }

    fn memory() -> Self::Keyring {
        Self::Keyring::memory()
    }

    fn create(
        keyring: &mut Self::Keyring,
        name: &str,
        password: Option<&str>,
    ) -> Result<KeyringEntry> {
        let (keystore, private_key) = keyring.create(name, password)?;
        Ok(KeyringEntry {
            public_key: Ext::extract_public_key(&keystore),
            address: keystore.address.clone(),
            name: keystore.name.clone(),
            secret: Some(Ext::display_secret(&private_key)),
        })
    }

    fn add_hex(
        keyring: &mut Self::Keyring,
        name: &str,
        hex: &str,
        password: Option<&str>,
    ) -> Result<KeyringEntry> {
        let private_key = Ext::import_hex(hex)?;
        let keystore = keyring.add(name, private_key.clone(), password)?;
        Ok(KeyringEntry {
            public_key: Ext::extract_public_key(&keystore),
            address: keystore.address.clone(),
            name: keystore.name.clone(),
            secret: Some(Ext::display_secret(&private_key)),
        })
    }

    fn import_suri(
        keyring: &mut Self::Keyring,
        name: &str,
        suri: &str,
        suri_password: Option<&str>,
        encryption_password: Option<&str>,
    ) -> Result<KeyringEntry> {
        let (keystore, private_key) =
            keyring.import_suri(name, suri, suri_password, encryption_password)?;
        Ok(KeyringEntry {
            public_key: Ext::extract_public_key(&keystore),
            address: keystore.address.clone(),
            name: keystore.name.clone(),
            secret: Some(Ext::display_secret(&private_key)),
        })
    }

    fn list(keyring: &Self::Keyring) -> Result<Vec<KeyringEntry>> {
        Ok(keyring
            .list()
            .iter()
            .map(|ks| KeyringEntry {
                public_key: Ext::extract_public_key(ks),
                address: ks.address.clone(),
                name: ks.name.clone(),
                secret: None,
            })
            .collect())
    }

    fn vanity(
        keyring: &mut Self::Keyring,
        name: &str,
        prefix: &str,
        password: Option<&str>,
    ) -> Result<KeyringEntry> {
        let normalized = Ext::normalize_prefix(prefix)?;
        let mut attempts = 0usize;
        let private_key = loop {
            if attempts >= MAX_VANITY_ATTEMPTS {
                anyhow::bail!("vanity search exceeded maximum attempts ({MAX_VANITY_ATTEMPTS})");
            }
            let candidate = C::random_private()?;
            if normalized.is_empty() || Ext::matches_vanity(&candidate, &normalized)? {
                break candidate;
            }
            attempts += 1;
        };
        let keystore = keyring.add(name, private_key.clone(), password)?;
        Ok(KeyringEntry {
            public_key: Ext::extract_public_key(&keystore),
            address: keystore.address.clone(),
            name: keystore.name.clone(),
            secret: Some(Ext::display_secret(&private_key)),
        })
    }
}
