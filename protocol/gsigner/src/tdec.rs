// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Threshold-decryption key storage.
//!
//! This module stores validator threshold-decryption private material separately
//! from signing schemes. It intentionally does not implement [`crate::CryptoScheme`]:
//! these keys create decryption shares, not signatures.

use crate::{
    error::{Result, SignerError},
    keyring::{self, KeystoreEntry},
};
use ferveo_common::{Keypair, PublicKey, from_bytes, to_bytes};
use gear_tdec::{
    BlindedKeyShare, CiphertextHeader, DecryptionShareSimple, DomainPoint,
    PublicDecryptionContextSimple, bls12_381::E,
};
use hex::ToHex;
use serde::{Deserialize, Serialize};
use std::{
    fmt,
    path::PathBuf,
    sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard},
};
use tempfile::TempDir;

pub type TdecPublicKey = PublicKey<E>;
pub type TdecKeypair = Keypair<E>;
pub type TdecDecryptionKey = DomainPoint<E>;
pub type TdecBlindedKeyShare = BlindedKeyShare<E>;
pub type TdecCiphertextHeader = CiphertextHeader<E>;
pub type TdecDecryptionShare = DecryptionShareSimple<E>;
pub type TdecPublicDecryptionContext = PublicDecryptionContextSimple<E>;

const NAMESPACE_TDEC: &str = "tdec";

/// JSON keystore entry for one validator threshold-decryption key.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TdecKeystore {
    pub name: String,
    pub public_key: String,
    pub validator_decryption_key: String,
}

impl TdecKeystore {
    fn from_keypair(name: &str, keypair: &TdecKeypair) -> Result<Self> {
        Ok(Self {
            name: name.to_string(),
            public_key: encode_public_key(&keypair.public_key())?,
            validator_decryption_key: encode_decryption_key(&keypair.decryption_key)?,
        })
    }

    fn public_key(&self) -> Result<TdecPublicKey> {
        decode_public_key(&self.public_key)
    }

    fn keypair(&self) -> Result<TdecKeypair> {
        Ok(TdecKeypair {
            decryption_key: decode_decryption_key(&self.validator_decryption_key)?,
        })
    }
}

impl KeystoreEntry for TdecKeystore {
    fn name(&self) -> &str {
        &self.name
    }

    fn set_name(&mut self, name: &str) {
        self.name = name.to_string();
    }
}

/// Store for validator threshold-decryption keys.
#[derive(Clone)]
pub struct TdecKeyStore {
    keyring: Arc<RwLock<keyring::Keyring<TdecKeystore>>>,
    _tmp_dir: Option<Arc<TempDir>>,
}

impl fmt::Debug for TdecKeyStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TdecKeyStore")
            .field("keys", &self.list_public_keys().ok())
            .finish()
    }
}

impl TdecKeyStore {
    pub fn new(keyring: keyring::Keyring<TdecKeystore>) -> Self {
        Self {
            keyring: Arc::new(RwLock::new(keyring)),
            _tmp_dir: None,
        }
    }

    fn with_tempdir(keyring: keyring::Keyring<TdecKeystore>, tmp_dir: Option<TempDir>) -> Self {
        Self {
            keyring: Arc::new(RwLock::new(keyring)),
            _tmp_dir: tmp_dir.map(Arc::new),
        }
    }

    pub fn memory() -> Self {
        let keyring = keyring::Keyring::try_memory().expect("memory keyring should not fail");
        Self::new(keyring)
    }

    pub fn fs(path: PathBuf) -> Result<Self> {
        let keyring = keyring::Keyring::load(Self::namespaced_path(path))?;
        Ok(Self::new(keyring))
    }

    pub fn fs_temporary() -> Result<Self> {
        let temp_dir = tempfile::tempdir()?;
        let keyring = keyring::Keyring::load(Self::namespaced_path(temp_dir.path().to_path_buf()))?;
        Ok(Self::with_tempdir(keyring, Some(temp_dir)))
    }

    pub fn namespaced_path(path: PathBuf) -> PathBuf {
        keyring::Keyring::<TdecKeystore>::namespaced_path(path, NAMESPACE_TDEC)
    }

    fn keyring(&self) -> Result<RwLockReadGuard<'_, keyring::Keyring<TdecKeystore>>> {
        self.keyring
            .read()
            .map_err(|err| SignerError::Other(format!("Failed to acquire read lock: {err}")))
    }

    fn keyring_mut(&self) -> Result<RwLockWriteGuard<'_, keyring::Keyring<TdecKeystore>>> {
        self.keyring
            .write()
            .map_err(|err| SignerError::Other(format!("Failed to acquire write lock: {err}")))
    }

    fn key_name(public_key: &TdecPublicKey) -> Result<String> {
        Ok(format!(
            "key-{}",
            public_key
                .to_bytes()
                .map_err(|err| SignerError::Serialization(err.to_string()))?
                .encode_hex::<String>()
        ))
    }

    /// Store a validator decryption scalar and return its public key.
    pub fn import_decryption_key(
        &self,
        validator_decryption_key: TdecDecryptionKey,
    ) -> Result<TdecPublicKey> {
        let keypair = TdecKeypair {
            decryption_key: validator_decryption_key,
        };
        self.import_keypair(keypair)
    }

    /// Store a full tdec keypair and return its public key.
    pub fn import_keypair(&self, keypair: TdecKeypair) -> Result<TdecPublicKey> {
        let public_key = keypair.public_key();
        let name = Self::key_name(&public_key)?;
        let keystore = TdecKeystore::from_keypair(&name, &keypair)?;
        self.keyring_mut()?.store(&name, keystore)?;
        Ok(public_key)
    }

    /// Get the private validator decryption scalar by public key.
    pub fn validator_decryption_key(
        &self,
        public_key: &TdecPublicKey,
    ) -> Result<TdecDecryptionKey> {
        Ok(self.keypair(public_key)?.decryption_key)
    }

    /// Get the full tdec keypair by public key.
    pub fn keypair(&self, public_key: &TdecPublicKey) -> Result<TdecKeypair> {
        let storage = self.keyring()?;
        for keystore in storage.list() {
            if keystore.public_key()? == *public_key {
                return keystore.keypair();
            }
        }
        Err(SignerError::KeyNotFound(format!("{public_key}")))
    }

    /// Create a decryption share using the local private key matching
    /// `public_context.validator_public_key`.
    pub fn create_share(
        &self,
        public_context: &TdecPublicDecryptionContext,
        ciphertext_header: &TdecCiphertextHeader,
        aad: &[u8],
    ) -> Result<TdecDecryptionShare> {
        self.create_share_with_blinded_key(
            &public_context.validator_public_key,
            &public_context.blinded_key_share,
            ciphertext_header,
            aad,
        )
    }

    /// Create a decryption share from explicit public key + blinded key share.
    pub fn create_share_with_blinded_key(
        &self,
        public_key: &TdecPublicKey,
        blinded_key_share: &TdecBlindedKeyShare,
        ciphertext_header: &TdecCiphertextHeader,
        aad: &[u8],
    ) -> Result<TdecDecryptionShare> {
        let keypair = self.keypair(public_key)?;
        blinded_key_share
            .create_decryption_share_simple(ciphertext_header, aad, &keypair)
            .map_err(|err| SignerError::Crypto(err.to_string()))
    }

    pub fn has_key(&self, public_key: &TdecPublicKey) -> Result<bool> {
        let storage = self.keyring()?;
        for keystore in storage.list() {
            if keystore.public_key()? == *public_key {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub fn list_public_keys(&self) -> Result<Vec<TdecPublicKey>> {
        self.keyring()?
            .list()
            .iter()
            .map(TdecKeystore::public_key)
            .collect()
    }

    pub fn clear_keys(&self) -> Result<()> {
        let mut storage = self.keyring_mut()?;
        let names: Vec<String> = storage
            .list()
            .iter()
            .map(|keystore| keystore.name().to_string())
            .collect();
        for name in names {
            storage.remove(&name)?;
        }
        Ok(())
    }
}

fn encode_public_key(public_key: &TdecPublicKey) -> Result<String> {
    Ok(hex::encode(public_key.to_bytes().map_err(|err| {
        SignerError::Serialization(err.to_string())
    })?))
}

fn decode_public_key(encoded: &str) -> Result<TdecPublicKey> {
    let bytes = hex::decode(encoded)?;
    TdecPublicKey::from_bytes(&bytes).map_err(|err| SignerError::InvalidKey(err.to_string()))
}

fn encode_decryption_key(key: &TdecDecryptionKey) -> Result<String> {
    Ok(hex::encode(to_bytes(key).map_err(|err| {
        SignerError::Serialization(err.to_string())
    })?))
}

fn decode_decryption_key(encoded: &str) -> Result<TdecDecryptionKey> {
    let bytes = hex::decode(encoded)?;
    from_bytes(&bytes).map_err(|err| SignerError::InvalidKey(err.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imports_and_gets_validator_decryption_key_by_public_key() {
        let mut rng = gear_tdec::rand_utils::test_rng();
        let keypair = TdecKeypair::new(&mut rng);
        let store = TdecKeyStore::memory();

        let public_key = store.import_keypair(keypair).unwrap();
        assert!(store.has_key(&public_key).unwrap());
        assert_eq!(
            store.validator_decryption_key(&public_key).unwrap(),
            keypair.decryption_key
        );
    }

    #[test]
    fn creates_decryption_share_from_public_context() {
        let mut rng = gear_tdec::rand_utils::test_rng();
        let dealer = gear_tdec::deal::<E>(3, 2, &mut rng);
        let context = dealer.private_contexts[0].clone();
        let public_context = context.public_decryption_contexts[context.index].clone();
        let ciphertext =
            gear_tdec::encrypt_raw::<E>(b"hello", b"aad", &dealer.public_key, &mut rng).unwrap();
        let header = ciphertext.header().unwrap();
        let store = TdecKeyStore::memory();
        store
            .import_decryption_key(context.validator_decryption_key)
            .unwrap();

        let expected = context.create_share(&header, b"aad").unwrap();
        let actual = store
            .create_share(&public_context, &header, b"aad")
            .unwrap();
        assert_eq!(actual, expected);
    }
}
