use crate::{scrypt::ScryptParams, KeyStorage};
use anyhow::{anyhow, Result};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use ethexe_common::ecdsa::{PrivateKey, PublicKey};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    str::FromStr,
    sync::RwLock,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
struct EncryptedKeyEntry {
    pub encrypted_data: String,
    pub scrypt_params: String,
    pub nonce: String,
    pub public_key: String,
    pub created_at: u64,
}

impl EncryptedKeyEntry {
    const NONCE_LENGTH: usize = 24;

    pub fn new(private_key: PrivateKey, password: &[u8]) -> Result<Self> {
        let scrypt_params = ScryptParams::new();
        let encryption_key = scrypt_params.derive_key(password)?;

        let mut nonce = [0; Self::NONCE_LENGTH];
        rand::thread_rng().fill_bytes(&mut nonce);

        let private_key_bytes: [u8; 32] = private_key.into();
        let encrypted_data = nacl::secret_box::pack(&private_key_bytes, &nonce, &encryption_key)
            .map_err(|e| anyhow!("Encryption failed: {e:?}"))?;

        let public_key = PublicKey::from_private(private_key);

        Ok(Self {
            encrypted_data: STANDARD.encode(&encrypted_data),
            scrypt_params: STANDARD.encode(scrypt_params.encode()),
            nonce: STANDARD.encode(nonce),
            public_key: public_key.to_hex(),
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        })
    }

    pub fn decrypt(&self, password: &[u8]) -> Result<PrivateKey> {
        let scrypt_params_bytes = STANDARD
            .decode(&self.scrypt_params)
            .map_err(|e| anyhow!("Scrypt params base64 decode failed: {e}"))?;

        if scrypt_params_bytes.len() != ScryptParams::ENCODED_LENGTH {
            return Err(anyhow!("Invalid scrypt params length"));
        }

        let mut scrypt_params_array = [0u8; ScryptParams::ENCODED_LENGTH];
        scrypt_params_array.copy_from_slice(&scrypt_params_bytes);
        let scrypt_params = ScryptParams::decode(scrypt_params_array);

        let encryption_key = scrypt_params.derive_key(password)?;

        let nonce_bytes = STANDARD
            .decode(&self.nonce)
            .map_err(|e| anyhow!("Nonce base64 decode failed: {e}"))?;

        if nonce_bytes.len() != Self::NONCE_LENGTH {
            return Err(anyhow!("Invalid nonce length"));
        }

        let mut nonce = [0u8; Self::NONCE_LENGTH];
        nonce.copy_from_slice(&nonce_bytes);

        let encrypted_data = STANDARD
            .decode(&self.encrypted_data)
            .map_err(|e| anyhow!("Encrypted data base64 decode failed: {e}"))?;

        let decrypted_data = nacl::secret_box::open(&encrypted_data, &nonce, &encryption_key)
            .map_err(|e| anyhow!("Decryption failed: {e:?}"))?;

        if decrypted_data.len() != 32 {
            return Err(anyhow!("Invalid private key length"));
        }

        let mut private_key_bytes = [0u8; 32];
        private_key_bytes.copy_from_slice(&decrypted_data);

        Ok(PrivateKey::from(private_key_bytes))
    }
}

#[derive(Debug)]
pub struct EncryptedFSKeyStorage {
    path: PathBuf,
    keys: RwLock<HashMap<String, String>>, // PublicKey hex -> filename mapping
}

impl EncryptedFSKeyStorage {
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref().to_path_buf();

        if !path.exists() {
            fs::create_dir_all(&path)
                .map_err(|e| anyhow!("Failed to create keystore directory: {e}"))?;
        }

        let mut storage = Self {
            path,
            keys: RwLock::new(HashMap::new()),
        };

        storage.load_keys()?;
        Ok(storage)
    }

    fn load_keys(&mut self) -> Result<()> {
        let mut keys = self.keys.write().unwrap();
        keys.clear();

        if !self.path.exists() {
            return Ok(());
        }

        for entry in fs::read_dir(&self.path)? {
            let entry = entry?;
            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                if let Ok(content) = fs::read_to_string(&path) {
                    if let Ok(key_entry) = serde_json::from_str::<EncryptedKeyEntry>(&content) {
                        if let Some(filename) = path.file_name().and_then(|s| s.to_str()) {
                            keys.insert(key_entry.public_key.clone(), filename.to_string());
                        }
                    }
                }
            }
        }

        Ok(())
    }

    fn get_filename(&self, public_key: PublicKey) -> String {
        format!("{}.json", public_key.to_hex())
    }

    fn get_key_path(&self, public_key: PublicKey) -> PathBuf {
        self.path.join(self.get_filename(public_key))
    }

    pub fn add_key_with_password(
        &mut self,
        private_key: PrivateKey,
        password: &[u8],
    ) -> Result<PublicKey> {
        let public_key = PublicKey::from_private(private_key);
        let key_entry = EncryptedKeyEntry::new(private_key, password)?;

        let key_path = self.get_key_path(public_key);
        let content = serde_json::to_string_pretty(&key_entry)
            .map_err(|e| anyhow!("JSON serialization failed: {e}"))?;

        fs::write(&key_path, content).map_err(|e| anyhow!("Failed to write key file: {e}"))?;

        let mut keys = self.keys.write().unwrap();
        keys.insert(public_key.to_hex(), self.get_filename(public_key));

        Ok(public_key)
    }

    pub fn get_private_key_with_password(
        &self,
        public_key: PublicKey,
        password: &[u8],
    ) -> Result<PrivateKey> {
        let key_path = self.get_key_path(public_key);

        if !key_path.exists() {
            return Err(anyhow!("Key not found"));
        }

        let content =
            fs::read_to_string(&key_path).map_err(|e| anyhow!("Failed to read key file: {e}"))?;

        let key_entry: EncryptedKeyEntry = serde_json::from_str(&content)
            .map_err(|e| anyhow!("JSON deserialization failed: {e}"))?;

        key_entry.decrypt(password)
    }
}

impl KeyStorage for EncryptedFSKeyStorage {
    fn empty() -> Self
    where
        Self: Sized,
    {
        Self::new(std::env::temp_dir().join("ethexe_encrypted_keystore"))
            .expect("Failed to create temporary encrypted keystore")
    }

    fn add_key(&mut self, _private_key: PrivateKey) -> Result<PublicKey> {
        Err(anyhow!(
            "EncryptedFSKeyStorage requires password. Use add_key_with_password instead."
        ))
    }

    fn get_private_key(&self, _public_key: PublicKey) -> Result<PrivateKey> {
        Err(anyhow!(
            "EncryptedFSKeyStorage requires password. Use get_private_key_with_password instead."
        ))
    }

    fn has_key(&self, public_key: PublicKey) -> Result<bool> {
        let keys = self.keys.read().unwrap();
        Ok(keys.contains_key(&public_key.to_hex()))
    }

    fn list_keys(&self) -> Result<Vec<PublicKey>> {
        let keys = self.keys.read().unwrap();
        let mut result = Vec::new();
        for key_hex in keys.keys() {
            if let Ok(public_key) = PublicKey::from_str(key_hex) {
                result.push(public_key);
            }
        }
        Ok(result)
    }

    fn clear_keys(&mut self) -> Result<()> {
        let keys_to_remove: Vec<PublicKey> = self.list_keys()?;

        for public_key in keys_to_remove {
            let key_path = self.get_key_path(public_key);
            if key_path.exists() {
                fs::remove_file(&key_path)
                    .map_err(|e| anyhow!("Failed to remove key file: {e}"))?;
            }
        }

        let mut keys = self.keys.write().unwrap();
        keys.clear();

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_encrypted_key_entry() {
        let private_key = PrivateKey::random();
        let password = b"test_password";

        let entry = EncryptedKeyEntry::new(private_key, password).unwrap();
        let decrypted = entry.decrypt(password).unwrap();

        assert_eq!(private_key, decrypted);
    }

    #[test]
    fn test_encrypted_key_entry_wrong_password() {
        let private_key = PrivateKey::random();
        let password = b"test_password";
        let wrong_password = b"wrong_password";

        let entry = EncryptedKeyEntry::new(private_key, password).unwrap();
        assert!(entry.decrypt(wrong_password).is_err());
    }

    #[test]
    fn test_encrypted_fs_key_storage() {
        let temp_dir = TempDir::new().unwrap();
        let mut storage = EncryptedFSKeyStorage::new(temp_dir.path()).unwrap();

        let private_key = PrivateKey::random();
        let password = b"test_password";

        let public_key = storage
            .add_key_with_password(private_key, password)
            .unwrap();
        let retrieved_key = storage
            .get_private_key_with_password(public_key, password)
            .unwrap();

        assert_eq!(private_key, retrieved_key);
        assert_eq!(storage.list_keys().unwrap(), vec![public_key]);

        storage.clear_keys().unwrap();
        assert!(storage.list_keys().unwrap().is_empty());
    }

    #[test]
    fn test_encrypted_fs_key_storage_persistence() {
        let temp_dir = TempDir::new().unwrap();
        let private_key = PrivateKey::random();
        let password = b"test_password";
        let public_key;

        {
            let mut storage = EncryptedFSKeyStorage::new(temp_dir.path()).unwrap();
            public_key = storage
                .add_key_with_password(private_key, password)
                .unwrap();
        }

        {
            let storage = EncryptedFSKeyStorage::new(temp_dir.path()).unwrap();
            let retrieved_key = storage
                .get_private_key_with_password(public_key, password)
                .unwrap();
            assert_eq!(private_key, retrieved_key);
        }
    }
}
