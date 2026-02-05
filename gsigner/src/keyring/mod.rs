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

//! Unified keyring manager supporting multiple cryptographic schemes.
//!
//! This module provides a top-level keyring abstraction that can manage keys
//! across different signature schemes by relying on scheme-specific keystore
//! types to implement [`KeystoreEntry`].
//!
//! # Storage Backends
//!
//! The keyring uses the [`StorageBackend`] trait for persistence.
//! Built-in backends include:
//! - [`FilesystemBackend`] - File-based storage
//! - [`MemoryBackend`] - In-memory storage (for testing)
//!
//! Custom backends can be implemented by implementing the `StorageBackend` trait.

use crate::{
    error,
    scheme::CryptoScheme,
    storage::{FilesystemBackend, MemoryBackend, StorageBackend, StorageError},
};
use anyhow::{Result, anyhow};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use rand::RngCore;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sp_core::crypto::Pair as PairTrait;
use std::{
    fs,
    marker::PhantomData,
    path::{Path, PathBuf},
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

/// Filename for keyring configuration.
pub const CONFIG_FILE: &str = "keyring.json";
const NONCE_LENGTH: usize = 24;
const KEY_SIZE: usize = 32;
const SCRYPT_LOG_N_MIN: u32 = 10;
const SCRYPT_LOG_N_MAX: u32 = 20;
const SCRYPT_R_MAX: u32 = 8;
const SCRYPT_P_MAX: u32 = 8;

pub const NAMESPACE_NET: &str = "net";
pub const NAMESPACE_SECP: &str = "secp";
pub const NAMESPACE_ED: &str = "ed";
pub const NAMESPACE_SR: &str = "sr";

/// Scrypt parameters used for deriving the symmetric key.
struct Scrypt {
    salt: [u8; 32],
    n: u32,
    r: u32,
    p: u32,
}

impl Scrypt {
    const ENCODED_LENGTH: usize = 44;

    fn encode(&self) -> [u8; Self::ENCODED_LENGTH] {
        let mut buf = [0u8; Self::ENCODED_LENGTH];
        let n: u32 = 1 << self.n;
        buf[..32].copy_from_slice(&self.salt);
        buf[32..36].copy_from_slice(&n.to_le_bytes());
        buf[36..40].copy_from_slice(&self.p.to_le_bytes());
        buf[40..44].copy_from_slice(&self.r.to_le_bytes());
        buf
    }

    fn decode(encoded: [u8; Self::ENCODED_LENGTH]) -> Result<Self> {
        let mut salt = [0u8; 32];
        salt.copy_from_slice(&encoded[..32]);

        let params = encoded[32..]
            .chunks(4)
            .map(|bytes| {
                let mut buf = [0u8; 4];
                buf.copy_from_slice(bytes);
                u32::from_le_bytes(buf)
            })
            .collect::<Vec<_>>();

        let (n_raw, p, r) = match (params.first(), params.get(1), params.get(2)) {
            (Some(&n_raw), Some(&p), Some(&r)) => (n_raw, p, r),
            _ => anyhow::bail!("Invalid scrypt parameter block"),
        };

        if !n_raw.is_power_of_two() {
            anyhow::bail!("Invalid scrypt N value (must be power of two)");
        }

        let n_log2 = n_raw.trailing_zeros();
        if !(SCRYPT_LOG_N_MIN..=SCRYPT_LOG_N_MAX).contains(&n_log2) {
            anyhow::bail!("Unsupported scrypt N: 2^{n_log2}");
        }

        if r == 0 || r > SCRYPT_R_MAX {
            anyhow::bail!("Unsupported scrypt r parameter");
        }

        if p == 0 || p > SCRYPT_P_MAX {
            anyhow::bail!("Unsupported scrypt p parameter");
        }

        Ok(Self {
            salt,
            n: n_log2,
            r,
            p,
        })
    }

    fn passwd(&self, passphrase: &[u8]) -> Result<[u8; KEY_SIZE]> {
        let mut passwd = [0u8; KEY_SIZE];
        let output = nacl::scrypt(
            passphrase,
            &self.salt,
            self.n as u8,
            self.r as usize,
            self.p as usize,
            KEY_SIZE,
            &|_| {},
        )
        .map_err(|e| anyhow!("{:?}", e))?;
        passwd.copy_from_slice(&output[..KEY_SIZE]);
        Ok(passwd)
    }
}

impl Default for Scrypt {
    fn default() -> Self {
        let mut salt = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut salt);

        Self {
            salt,
            n: 15,
            r: 8,
            p: 1,
        }
    }
}

/// Encrypt arbitrary bytes with scrypt + xsalsa20-poly1305.
pub fn encrypt_secret(plaintext: &[u8], passphrase: &[u8]) -> Result<String> {
    let scrypt = Scrypt::default();
    let passwd = scrypt.passwd(passphrase)?;

    let mut encoded = Vec::with_capacity(Scrypt::ENCODED_LENGTH + NONCE_LENGTH + plaintext.len());
    encoded.extend_from_slice(&scrypt.encode());

    let mut nonce = [0u8; NONCE_LENGTH];
    rand::thread_rng().fill_bytes(&mut nonce);
    encoded.extend_from_slice(&nonce);

    let ciphertext = nacl::secret_box::pack(plaintext, &nonce, &passwd[..KEY_SIZE])
        .map_err(|e| anyhow!("{:?}", e))?;
    encoded.extend_from_slice(&ciphertext);

    Ok(STANDARD.encode(encoded))
}

/// Decrypt bytes previously produced by [`encrypt_secret`].
pub fn decrypt_secret(encoded: &str, passphrase: &[u8]) -> Result<Vec<u8>> {
    let decoded = STANDARD.decode(encoded)?;
    if decoded.len() < Scrypt::ENCODED_LENGTH + NONCE_LENGTH {
        anyhow::bail!("Invalid encrypted payload");
    }

    let mut scrypt_bytes = [0u8; Scrypt::ENCODED_LENGTH];
    scrypt_bytes.copy_from_slice(&decoded[..Scrypt::ENCODED_LENGTH]);
    let scrypt = Scrypt::decode(scrypt_bytes)?;
    let passwd = scrypt.passwd(passphrase)?;

    let encrypted = &decoded[Scrypt::ENCODED_LENGTH..];
    let nonce = &encrypted[..NONCE_LENGTH];
    let payload = &encrypted[NONCE_LENGTH..];

    let secret = nacl::secret_box::open(payload, nonce, &passwd[..KEY_SIZE])
        .map_err(|e| anyhow!("{:?}", e))?;
    Ok(secret)
}

/// Trait for private keys that can derive public keys and be randomly generated.
pub trait PrivateKeyOps: Clone {
    type PublicKey;

    /// Derive the public key from this private key.
    fn public_key(&self) -> Self::PublicKey;

    /// Generate a new random private key.
    fn random() -> Self;
}

/// Trait for public keys that can be serialized to/from bytes.
pub trait PublicKeyBytes: Clone {
    /// Serialize to fixed-size bytes.
    fn to_bytes(&self) -> [u8; 32];

    /// Deserialize from fixed-size bytes.
    fn from_bytes(bytes: [u8; 32]) -> Self;
}

/// Helper module with default implementations for KeyCodec methods.
/// These are used by schemes that follow the standard hex-encoding pattern.
pub mod codec_defaults {
    use super::*;

    /// Default implementation for `derive_public` using `PrivateKeyOps`.
    pub fn derive_public<P: PrivateKeyOps>(private_key: &P) -> P::PublicKey {
        private_key.public_key()
    }

    /// Default implementation for `random_private` using `PrivateKeyOps`.
    pub fn random_private<P: PrivateKeyOps>() -> Result<P> {
        Ok(P::random())
    }

    /// Default implementation for `encode_public` using `PublicKeyBytes`.
    pub fn encode_public<P: PublicKeyBytes>(public_key: &P) -> Result<String> {
        Ok(hex::encode(public_key.to_bytes()))
    }

    /// Default implementation for `decode_public` using `PublicKeyBytes`.
    pub fn decode_public<P: PublicKeyBytes>(encoded: &str) -> Result<P> {
        let bytes = hex::decode(encoded)?;
        if bytes.len() != 32 {
            return Err(anyhow!(
                "Invalid public key length: expected 32, got {}",
                bytes.len()
            ));
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&bytes);
        Ok(P::from_bytes(arr))
    }

    /// Default implementation for `encode_address` using SS58 format.
    #[cfg(any(feature = "sr25519", feature = "ed25519"))]
    pub fn encode_ss58_address(address: &crate::address::SubstrateAddress) -> Result<String> {
        Ok(address.as_ss58().to_string())
    }

    /// Default implementation for `decode_address` using SS58 format.
    #[cfg(any(feature = "sr25519", feature = "ed25519"))]
    pub fn decode_ss58_address(encoded: &str) -> Result<crate::address::SubstrateAddress> {
        crate::address::SubstrateAddress::from_ss58(encoded)
            .map_err(|err| anyhow!("Invalid SS58 address: {err}"))
    }
}

/// Trait describing how to convert to and from string representations for key material,
/// and how to generate/import keys for keyring operations.
pub trait KeyCodec {
    /// Concrete Substrate pair type.
    type Pair: PairTrait;
    /// Private key wrapper type.
    type PrivateKey: Clone;
    /// Public key wrapper type.
    type PublicKey: Clone;
    /// Address type exposed by the scheme.
    type Address;

    /// Human-readable key type identifier (e.g., "ecdsa", "sr25519", "ed25519").
    const KEY_TYPE: &'static str;

    /// Derive the public key from the provided private key.
    fn derive_public(private_key: &Self::PrivateKey) -> Self::PublicKey;

    /// Derive the address from the provided public key.
    fn derive_address(public_key: &Self::PublicKey) -> Result<Self::Address>;

    /// Encode private key for storage.
    fn encode_private(private_key: &Self::PrivateKey) -> Result<String>;

    /// Decode private key from storage.
    fn decode_private(encoded: &str) -> Result<Self::PrivateKey>;

    /// Encode public key for storage.
    fn encode_public(public_key: &Self::PublicKey) -> Result<String>;

    /// Decode public key from storage.
    fn decode_public(encoded: &str) -> Result<Self::PublicKey>;

    /// Encode address for storage.
    fn encode_address(address: &Self::Address) -> Result<String>;

    /// Decode address from storage.
    fn decode_address(encoded: &str) -> Result<Self::Address>;

    /// Generate a new random private key.
    fn random_private() -> Result<Self::PrivateKey>;

    /// Import a private key from a SURI (mnemonic/derivation path).
    fn import_suri(suri: &str, password: Option<&str>) -> Result<Self::PrivateKey>;
}

/// Generic keystore structure compatible with the CLI keyring workflow.
#[derive(Serialize, Deserialize)]
#[serde(bound(serialize = "", deserialize = ""))]
pub struct SubstrateKeystore<C: KeyCodec> {
    /// Human readable key name.
    pub name: String,
    /// Encoded public key.
    pub public_key: String,
    /// Encoded address.
    pub address: String,
    /// Encoded private key.
    pub private_key: String,
    /// Encryption metadata if the private key was stored securely.
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encryption: Option<SecretEncryption>,
    #[serde(default)]
    pub meta: SubstrateKeystoreMeta<C>,
    #[serde(skip)]
    _marker: PhantomData<C>,
}

impl<C: KeyCodec> SubstrateKeystore<C> {
    /// Build a keystore entry from a private key.
    pub fn from_private_key(name: &str, private_key: C::PrivateKey) -> Result<Self> {
        Self::from_private_key_with_password(name, private_key, None)
    }

    /// Build a keystore entry from a private key with optional encryption.
    pub fn from_private_key_with_password(
        name: &str,
        private_key: C::PrivateKey,
        password: Option<&str>,
    ) -> Result<Self> {
        let public_key = C::derive_public(&private_key);
        let address = C::derive_address(&public_key)?;
        let encoded_private = C::encode_private(&private_key)?;
        let (secret, encryption) = if let Some(password) = password {
            let encrypted = encrypt_secret(encoded_private.as_bytes(), password.as_bytes())?;
            (encrypted, Some(SecretEncryption::scrypt()))
        } else {
            (encoded_private, None)
        };
        Ok(Self {
            name: name.to_string(),
            public_key: C::encode_public(&public_key)?,
            address: C::encode_address(&address)?,
            private_key: secret,
            encryption,
            meta: SubstrateKeystoreMeta::default(),
            _marker: PhantomData,
        })
    }

    /// Decode the stored private key.
    pub fn private_key(&self) -> Result<C::PrivateKey> {
        self.private_key_with_password(None)
    }

    /// Decode the stored private key using the provided password.
    pub fn private_key_with_password(&self, password: Option<&str>) -> Result<C::PrivateKey> {
        if self.encryption.is_some() {
            let password = password.ok_or_else(|| {
                anyhow!("Password required for encrypted keystore '{}'", self.name())
            })?;
            let decrypted = decrypt_secret(&self.private_key, password.as_bytes())?;
            let encoded = String::from_utf8(decrypted)
                .map_err(|_| anyhow!("Invalid encrypted private key data"))?;
            return C::decode_private(&encoded);
        }

        C::decode_private(&self.private_key)
    }

    /// Decode the stored public key.
    pub fn public_key(&self) -> Result<C::PublicKey> {
        C::decode_public(&self.public_key)
    }

    /// Decode the stored address.
    pub fn address(&self) -> Result<C::Address> {
        C::decode_address(&self.address)
    }
}

impl<C: KeyCodec> Default for SubstrateKeystore<C> {
    fn default() -> Self {
        Self {
            name: String::new(),
            public_key: String::new(),
            address: String::new(),
            private_key: String::new(),
            encryption: None,
            meta: SubstrateKeystoreMeta::default(),
            _marker: PhantomData,
        }
    }
}

/// Metadata describing how a private key is encrypted.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SecretEncryption {
    #[serde(rename = "type")]
    pub ty: String,
}

impl SecretEncryption {
    pub fn scrypt() -> Self {
        Self {
            ty: "scrypt-xsalsa20-poly1305".into(),
        }
    }
}

/// Metadata stored alongside keystore entries.
#[derive(Serialize, Deserialize)]
#[serde(bound(serialize = "", deserialize = ""))]
pub struct SubstrateKeystoreMeta<C: KeyCodec> {
    #[serde(rename = "whenCreated")]
    pub when_created: u128,
    #[serde(
        rename = "keyType",
        default = "SubstrateKeystoreMeta::<C>::default_key_type"
    )]
    pub key_type: String,
    #[serde(skip)]
    _marker: PhantomData<C>,
}

impl<C: KeyCodec> Default for SubstrateKeystoreMeta<C> {
    fn default() -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        Self {
            when_created: now,
            key_type: Self::default_key_type(),
            _marker: PhantomData,
        }
    }
}

impl<C: KeyCodec> SubstrateKeystoreMeta<C> {
    fn default_key_type() -> String {
        C::KEY_TYPE.to_string()
    }
}

impl<C: KeyCodec> Clone for SubstrateKeystore<C> {
    fn clone(&self) -> Self {
        Self {
            name: self.name.clone(),
            public_key: self.public_key.clone(),
            address: self.address.clone(),
            private_key: self.private_key.clone(),
            encryption: self.encryption.clone(),
            meta: self.meta.clone(),
            _marker: PhantomData,
        }
    }
}

impl<C: KeyCodec> Clone for SubstrateKeystoreMeta<C> {
    fn clone(&self) -> Self {
        Self {
            when_created: self.when_created,
            key_type: self.key_type.clone(),
            _marker: PhantomData,
        }
    }
}

impl<C: KeyCodec> KeystoreEntry for SubstrateKeystore<C> {
    fn name(&self) -> &str {
        &self.name
    }

    fn set_name(&mut self, name: &str) {
        self.name = name.to_string();
    }
}

/// Signature schemes that can be stored in the JSON keyring.
pub trait KeyringScheme: CryptoScheme {
    /// Concrete keystore representation for this scheme.
    type Keystore: KeystoreEntry + Clone + Serialize + DeserializeOwned + Send + Sync + 'static;

    /// Directory namespace used to segregate scheme keyrings on disk.
    fn namespace() -> &'static str;

    /// Build a keystore representation from a private key.
    fn keystore_from_private(
        name: &str,
        private_key: &Self::PrivateKey,
        password: Option<&str>,
    ) -> error::Result<Self::Keystore>;

    /// Recover the private key from a keystore.
    fn keystore_private(
        keystore: &Self::Keystore,
        password: Option<&str>,
    ) -> error::Result<Self::PrivateKey>;

    /// Recover the public key from a keystore.
    fn keystore_public(keystore: &Self::Keystore) -> error::Result<Self::PublicKey>;

    /// Recover the address from a keystore.
    fn keystore_address(keystore: &Self::Keystore) -> error::Result<Self::Address>;
}

/// Trait for keystore types that can be used with the keyring.
pub trait KeystoreEntry: Serialize + for<'de> Deserialize<'de> + Clone {
    /// Get the name/identifier of this keystore entry.
    fn name(&self) -> &str;

    /// Set the name of this keystore entry.
    fn set_name(&mut self, name: &str);
}

/// Keyring configuration stored on disk.
#[derive(Default, Serialize, Deserialize)]
struct KeyringConfig {
    /// The primary key name (if set).
    primary: Option<String>,
}

/// Convert StorageError to anyhow::Error
fn storage_err(e: StorageError) -> anyhow::Error {
    anyhow::Error::msg(e.to_string())
}

/// Unified keyring manager for cryptographic keys.
///
/// Manages a collection of serialized keystores with a primary key concept.
/// The keystore format is delegated to the scheme-specific implementation via
/// the [`KeystoreEntry`] trait.
///
/// # Storage Backends
///
/// The keyring can use any implementation of [`StorageBackend`].
/// Use [`Keyring::with_backend`] to create a keyring with a custom backend.
///
/// # Directory Structure (Filesystem Backend)
///
/// ```text
/// keyring/
/// ├── keyring.json          # Configuration (primary key)
/// ├── alice.json            # Individual keystores
/// ├── bob.json
/// └── ...
/// ```
pub struct Keyring<K: KeystoreEntry> {
    backend: Arc<dyn StorageBackend>,
    keystores: Vec<K>,
    primary: Option<String>,
}

impl<K: KeystoreEntry> Clone for Keyring<K> {
    fn clone(&self) -> Self {
        Self {
            backend: Arc::clone(&self.backend),
            keystores: self.keystores.clone(),
            primary: self.primary.clone(),
        }
    }
}

fn resolve_namespaced_path_impl(store: PathBuf, namespace: &str) -> PathBuf {
    if path_has_keyring(&store) || store.file_name().is_some_and(|name| name == namespace) {
        store
    } else {
        store.join(namespace)
    }
}

/// Resolve a storage path into a namespaced keyring directory.
///
/// This helper can be used without instantiating a [`Keyring`] to compute the
/// scheme-specific directory that should back the JSON keystore.
pub fn resolve_namespaced_path(store: PathBuf, namespace: &str) -> PathBuf {
    resolve_namespaced_path_impl(store, namespace)
}

impl<K: KeystoreEntry> Keyring<K> {
    /// Create a keyring with a custom storage backend.
    ///
    /// This is the most flexible constructor, allowing any [`StorageBackend`]
    /// implementation to be used.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use gsigner::{MemoryBackend, keyring::Keyring};
    ///
    /// let backend = MemoryBackend::new();
    /// let keyring = Keyring::<MyKeystore>::with_backend(backend)?;
    /// ```
    pub fn with_backend<B: StorageBackend + 'static>(backend: B) -> Result<Self> {
        Self::from_backend(Arc::new(backend))
    }

    /// Load keyring from directory.
    ///
    /// Creates the directory if it doesn't exist and loads all keystores from disk.
    pub fn load(store: PathBuf) -> Result<Self> {
        let backend = FilesystemBackend::new(store).map_err(storage_err)?;
        Self::from_backend(Arc::new(backend))
    }

    /// Create an in-memory keyring.
    pub fn memory() -> Self {
        Self::try_memory().unwrap_or_else(|_| Self {
            backend: Arc::new(MemoryBackend::new()),
            keystores: Vec::new(),
            primary: None,
        })
    }

    /// Fallible constructor for an in-memory keyring.
    pub fn try_memory() -> Result<Self> {
        Self::from_backend(Arc::new(MemoryBackend::new()))
    }

    fn from_backend(backend: Arc<dyn StorageBackend>) -> Result<Self> {
        let mut keystores = Vec::new();
        for (name, bytes) in backend.list_entries().map_err(storage_err)? {
            match Self::decode_keystore(&bytes, Some(&name)) {
                Ok(keystore) => keystores.push(keystore),
                Err(err) => tracing::warn!("Failed to load keystore '{name}': {err}"),
            }
        }

        let primary = if let Some(config_bytes) = backend.read_config().map_err(storage_err)? {
            let config: KeyringConfig = serde_json::from_slice(&config_bytes)?;
            config.primary
        } else {
            None
        };

        Ok(Self {
            backend,
            keystores,
            primary,
        })
    }

    /// Resolve a storage path into a namespaced keyring directory.
    ///
    /// This allows callers to pass a common root (e.g. `/keys`) while keeping
    /// scheme-specific keyrings separate (`/keys/secp`, `/keys/ed`, `/keys/net`, ...).
    /// If the provided path already contains keystores or configuration, it is returned
    /// unchanged to preserve existing data.
    pub fn namespaced_path(store: PathBuf, namespace: &str) -> PathBuf {
        resolve_namespaced_path_impl(store, namespace)
    }

    fn decode_keystore(bytes: &[u8], inferred_name: Option<&str>) -> Result<K> {
        let mut keystore: K = serde_json::from_slice(bytes)?;

        if let Some(stem) = inferred_name
            && keystore.name().is_empty()
        {
            keystore.set_name(stem);
        }

        Ok(keystore)
    }

    fn read_keystore_from_path(path: &Path) -> Result<K> {
        let bytes = fs::read(path)?;
        let inferred = path.file_stem().and_then(|s| s.to_str());
        Self::decode_keystore(&bytes, inferred)
    }

    /// Save keyring configuration to disk.
    fn save_config(&self) -> Result<()> {
        let config = KeyringConfig {
            primary: self.primary.clone(),
        };
        let bytes = serde_json::to_vec_pretty(&config)?;
        self.backend.write_config(&bytes).map_err(storage_err)?;
        Ok(())
    }

    /// Persist a keystore entry in the keyring.
    ///
    /// Saves the keystore to disk, overwriting any existing entry with the same name.
    pub fn store(&mut self, name: &str, mut keystore: K) -> Result<K> {
        keystore.set_name(name);

        let bytes = serde_json::to_vec_pretty(&keystore)?;
        self.backend
            .write_entry(name, &bytes)
            .map_err(storage_err)?;

        if let Some(index) = self.keystores.iter().position(|entry| entry.name() == name) {
            self.keystores[index] = keystore.clone();
        } else {
            self.keystores.push(keystore.clone());
        }

        Ok(keystore)
    }

    /// Import a keystore from an arbitrary JSON file.
    ///
    /// The file is deserialized, optionally renamed from its filename, and stored
    /// in the keyring directory.
    pub fn import(&mut self, path: PathBuf) -> Result<K> {
        let mut keystore = Self::read_keystore_from_path(&path)?;
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| anyhow!("Invalid file name"))?;

        keystore.set_name(name);
        self.store(name, keystore)
    }

    /// Get the primary keystore.
    ///
    /// Returns an error if no primary key is set or if the keyring is empty.
    pub fn primary(&mut self) -> Result<&K> {
        if self.keystores.is_empty() {
            return Err(anyhow!("No keys in keyring"));
        }

        if self.primary.is_none() {
            let first = self.keystores[0].name().to_string();
            self.primary = Some(first);
            self.save_config()?;
        }

        let primary_name = self
            .primary
            .as_ref()
            .ok_or_else(|| anyhow!("Primary key is not set"))?;
        self.keystores
            .iter()
            .find(|k| k.name() == primary_name)
            .ok_or_else(|| anyhow!("Primary key '{}' not found in keyring", primary_name))
    }

    /// Set the primary key by name.
    pub fn set_primary(&mut self, name: &str) -> Result<()> {
        if !self.keystores.iter().any(|k| k.name() == name) {
            return Err(anyhow!("Key '{}' not found in keyring", name));
        }

        self.primary = Some(name.to_string());
        self.save_config()?;
        Ok(())
    }

    /// List all keystores in the keyring.
    pub fn list(&self) -> &[K] {
        &self.keystores
    }

    /// Get a keystore by name.
    pub fn get(&self, name: &str) -> Option<&K> {
        self.keystores.iter().find(|k| k.name() == name)
    }

    /// Remove a keystore by name.
    pub fn remove(&mut self, name: &str) -> Result<K> {
        let index = self
            .keystores
            .iter()
            .position(|k| k.name() == name)
            .ok_or_else(|| anyhow!("Key '{}' not found", name))?;

        let keystore = self.keystores.remove(index);

        // Remove from storage backend
        self.backend.remove_entry(name).map_err(storage_err)?;

        // Clear primary if it was the removed key
        if self.primary.as_deref() == Some(name) {
            self.primary = None;
            self.save_config()?;
        }

        Ok(keystore)
    }

    /// Get the underlying storage backend.
    ///
    /// This can be useful for advanced operations or introspection.
    pub fn backend(&self) -> &dyn StorageBackend {
        self.backend.as_ref()
    }
}

fn path_has_keyring(path: &Path) -> bool {
    if path.join(CONFIG_FILE).exists() {
        return true;
    }

    fs::read_dir(path)
        .map(|entries| {
            entries.flatten().any(|entry| {
                let file_path = entry.path();
                file_path.is_file() && file_path.extension().is_some_and(|ext| ext == "json")
            })
        })
        .unwrap_or(false)
}

impl<C: KeyCodec> Keyring<SubstrateKeystore<C>> {
    /// Add an existing private key to the keyring.
    pub fn add(
        &mut self,
        name: &str,
        private_key: C::PrivateKey,
        password: Option<&str>,
    ) -> Result<SubstrateKeystore<C>> {
        let keystore =
            SubstrateKeystore::from_private_key_with_password(name, private_key, password)?;
        self.store(name, keystore)
    }

    /// Add a private key from its hex-encoded representation.
    pub fn add_hex(
        &mut self,
        name: &str,
        encoded: &str,
        password: Option<&str>,
    ) -> Result<SubstrateKeystore<C>> {
        let private_key = C::decode_private(encoded)?;
        self.add(name, private_key, password)
    }

    /// Generate and store a new random private key.
    pub fn create(
        &mut self,
        name: &str,
        password: Option<&str>,
    ) -> Result<(SubstrateKeystore<C>, C::PrivateKey)> {
        let private_key = C::random_private()?;
        let keystore = self.add(name, private_key.clone(), password)?;
        Ok((keystore, private_key))
    }

    /// Import a private key from a Substrate SURI (mnemonic, derivation path, etc.).
    pub fn import_suri(
        &mut self,
        name: &str,
        suri: &str,
        suri_password: Option<&str>,
        encryption_password: Option<&str>,
    ) -> Result<(SubstrateKeystore<C>, C::PrivateKey)> {
        let private_key = C::import_suri(suri, suri_password)?;
        let keystore = self.add(name, private_key.clone(), encryption_password)?;
        Ok((keystore, private_key))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};
    use std::{collections::HashSet, fs};

    #[derive(Clone, Serialize, Deserialize)]
    struct TestKeystore {
        name: String,
        data: String,
    }

    impl KeystoreEntry for TestKeystore {
        fn name(&self) -> &str {
            &self.name
        }

        fn set_name(&mut self, name: &str) {
            self.name = name.to_string();
        }
    }

    #[test]
    fn test_keyring_operations() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut keyring = Keyring::<TestKeystore>::load(temp_dir.path().to_path_buf()).unwrap();

        // Add keystores
        let key1 = TestKeystore {
            name: String::new(),
            data: "secret1".to_string(),
        };
        let key2 = TestKeystore {
            name: String::from("bob"),
            data: "secret2".to_string(),
        };

        keyring.store("alice", key1).unwrap();
        keyring.store("bob", key2).unwrap();

        // List
        assert_eq!(keyring.list().len(), 2);
        assert_eq!(
            keyring
                .list()
                .iter()
                .map(|k| k.name())
                .collect::<HashSet<_>>(),
            HashSet::from(["alice", "bob"])
        );

        // Get
        assert!(keyring.get("alice").is_some());
        assert!(keyring.get("charlie").is_none());

        // Set primary
        keyring.set_primary("alice").unwrap();
        assert_eq!(keyring.primary.as_deref(), Some("alice"));
        keyring.primary().unwrap();

        // Remove
        keyring.remove("alice").unwrap();
        assert_eq!(keyring.list().len(), 1);
        assert!(keyring.primary.is_none());
    }

    #[test]
    fn test_keyring_with_custom_backend() {
        let backend = MemoryBackend::new();
        let mut keyring = Keyring::<TestKeystore>::with_backend(backend).unwrap();

        let key = TestKeystore {
            name: String::new(),
            data: "secret".to_string(),
        };

        keyring.store("test", key).unwrap();
        assert_eq!(keyring.list().len(), 1);
    }

    #[test]
    fn namespaced_path_defaults_to_namespace() {
        let temp_dir = tempfile::tempdir().unwrap();
        let root = temp_dir.path().join("keys");
        fs::create_dir_all(&root).unwrap();

        let resolved = Keyring::<TestKeystore>::namespaced_path(root.clone(), NAMESPACE_SECP);
        assert_eq!(resolved, root.join(NAMESPACE_SECP));
    }

    #[test]
    fn namespaced_path_preserves_existing_store() {
        let temp_dir = tempfile::tempdir().unwrap();
        let root = temp_dir.path().join("keys");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("alice.json"), "{}").unwrap();

        let resolved = Keyring::<TestKeystore>::namespaced_path(root.clone(), NAMESPACE_ED);
        assert_eq!(resolved, root);
    }

    #[test]
    fn namespaced_path_prefers_existing_namespace() {
        let temp_dir = tempfile::tempdir().unwrap();
        let root = temp_dir.path().join("keys");
        let namespaced = root.join(NAMESPACE_NET);
        fs::create_dir_all(&namespaced).unwrap();
        fs::write(namespaced.join("alice.json"), "{}").unwrap();

        let resolved = Keyring::<TestKeystore>::namespaced_path(root, NAMESPACE_NET);
        assert!(resolved.ends_with(NAMESPACE_NET));
        assert!(resolved.join("alice.json").exists());
    }
}
