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

//! Command handlers for gsigner CLI.
//!
//! These handlers execute the commands and return structured results.
//! They don't print directly, allowing integrators to customize output formatting.

use super::commands::*;
use anyhow::Result;
use std::path::PathBuf;

/// Result of key generation
#[derive(Debug, Clone)]
pub struct KeyGenerationResult {
    pub public_key: String,
    pub address: String,
}

/// Result of key import
#[derive(Debug, Clone)]
pub struct KeyImportResult {
    pub public_key: String,
    pub address: String,
}

/// Result of signing operation
#[derive(Debug, Clone)]
pub struct SignResult {
    pub signature: String,
}

/// Result of verification operation
#[derive(Debug, Clone)]
pub struct VerifyResult {
    pub valid: bool,
}

/// Result of address conversion
#[derive(Debug, Clone)]
pub struct AddressResult {
    pub address: String,
}

/// Result of listing keys
#[derive(Debug, Clone)]
pub struct ListKeysResult {
    pub keys: Vec<KeyInfo>,
}

/// Information about a key
#[derive(Debug, Clone)]
pub struct KeyInfo {
    pub public_key: String,
    pub address: String,
}

/// Result of keyring operations
#[derive(Debug, Clone)]
pub struct KeyringResult {
    pub message: String,
    pub details: Option<KeyringDetails>,
}

/// Details about keyring operation
#[derive(Debug, Clone)]
pub struct KeyringDetails {
    pub name: String,
    pub public_key: String,
    pub address: String,
    pub keystore_name: Option<String>,
}

/// Result of keyring list operation
#[derive(Debug, Clone)]
pub struct KeyringListResult {
    pub keystores: Vec<KeystoreInfo>,
}

/// Information about a keystore
#[derive(Debug, Clone)]
pub struct KeystoreInfo {
    pub name: String,
    pub public_key: Option<String>,
    pub address: String,
    pub created: String,
}

/// Execute a gsigner command
pub fn execute_command(command: GSignerCommands) -> Result<CommandResult> {
    match command {
        GSignerCommands::Secp256k1 { command } => {
            let result = execute_secp256k1_command(command)?;
            Ok(CommandResult::Secp256k1(result))
        }
        GSignerCommands::Sr25519 { command } => {
            let result = execute_sr25519_command(command)?;
            Ok(CommandResult::Sr25519(result))
        }
    }
}

/// Result of command execution
#[derive(Debug, Clone)]
pub enum CommandResult {
    Secp256k1(Secp256k1Result),
    Sr25519(Sr25519Result),
}

/// Result of secp256k1 command
#[derive(Debug, Clone)]
pub enum Secp256k1Result {
    Generate(KeyGenerationResult),
    Sign(SignResult),
    Verify(VerifyResult),
    Address(AddressResult),
    List(ListKeysResult),
}

/// Result of sr25519 command
#[derive(Debug, Clone)]
pub enum Sr25519Result {
    Generate(KeyGenerationResult),
    Import(KeyImportResult),
    Sign(SignResult),
    Verify(VerifyResult),
    Address(AddressResult),
    Keyring(KeyringResult),
    KeyringList(KeyringListResult),
    List(ListKeysResult),
}

#[cfg(feature = "secp256k1")]
pub fn execute_secp256k1_command(command: Secp256k1Commands) -> Result<Secp256k1Result> {
    use crate::{
        Address, SignatureScheme, Signer,
        schemes::secp256k1::{PublicKey, Secp256k1, Secp256k1SignerExt, Signature},
    };

    match command {
        Secp256k1Commands::Generate { storage } => {
            let signer: Signer<Secp256k1> = create_signer(storage);
            let public_key = signer.generate_key()?;
            let address = signer.address(public_key);

            Ok(Secp256k1Result::Generate(KeyGenerationResult {
                public_key: public_key.to_hex(),
                address: format!("0x{}", hex::encode(address)),
            }))
        }
        Secp256k1Commands::Sign {
            public_key,
            data,
            storage,
            contract,
        } => {
            let signer: Signer<Secp256k1> = create_signer(storage);
            let public_key: PublicKey = public_key.parse()?;
            let data_bytes = hex::decode(&data)?;

            let signature = if let Some(contract_addr) = contract {
                let contract_bytes = hex::decode(contract_addr.trim_start_matches("0x"))?;
                let mut contract_array = [0u8; 20];
                contract_array.copy_from_slice(&contract_bytes);
                let contract_address = Address(contract_array);
                let signature =
                    signer.sign_for_contract(contract_address, public_key, &data_bytes)?;
                hex::encode(signature.into_pre_eip155_bytes())
            } else {
                let signature = signer.sign(public_key, &data_bytes)?;
                hex::encode(signature.into_pre_eip155_bytes())
            };

            Ok(Secp256k1Result::Sign(SignResult { signature }))
        }
        Secp256k1Commands::Verify {
            public_key,
            data,
            signature,
        } => {
            let public_key: PublicKey = public_key.parse()?;
            let data_bytes = hex::decode(&data)?;
            let sig_bytes = hex::decode(&signature)?;
            let mut sig_arr = [0u8; 65];
            sig_arr.copy_from_slice(&sig_bytes);
            let signature = Signature::from_pre_eip155_bytes(sig_arr)
                .ok_or_else(|| anyhow::anyhow!("Invalid signature"))?;

            <Secp256k1 as SignatureScheme>::verify(&public_key, &data_bytes, &signature)?;

            Ok(Secp256k1Result::Verify(VerifyResult { valid: true }))
        }
        Secp256k1Commands::Address { public_key } => {
            let public_key: PublicKey = public_key.parse()?;
            let address = public_key.to_address();

            Ok(Secp256k1Result::Address(AddressResult {
                address: format!("0x{}", hex::encode(address)),
            }))
        }
        Secp256k1Commands::List { storage } => {
            let signer: Signer<Secp256k1> = create_signer(storage);
            let keys = signer.list_keys()?;

            let key_infos: Vec<KeyInfo> = keys
                .into_iter()
                .map(|key| {
                    let address = signer.address(key);
                    KeyInfo {
                        public_key: key.to_hex(),
                        address: format!("0x{}", hex::encode(address)),
                    }
                })
                .collect();

            Ok(Secp256k1Result::List(ListKeysResult { keys: key_infos }))
        }
    }
}

#[cfg(not(feature = "secp256k1"))]
pub fn execute_secp256k1_command(_command: Secp256k1Commands) -> Result<Secp256k1Result> {
    anyhow::bail!("secp256k1 feature is not enabled. Rebuild with --features secp256k1");
}

#[cfg(feature = "sr25519")]
pub fn execute_sr25519_command(command: Sr25519Commands) -> Result<Sr25519Result> {
    use crate::{
        SignatureScheme, Signer,
        schemes::sr25519::{PublicKey, Signature, Sr25519, Sr25519SignerExt},
    };

    match command {
        Sr25519Commands::Generate { storage } => {
            let signer: Signer<Sr25519> = create_signer(storage);
            let public_key = signer.generate_key()?;
            let address = signer.address(public_key);

            Ok(Sr25519Result::Generate(KeyGenerationResult {
                public_key: hex::encode(public_key.to_bytes()),
                address: address.as_ss58().to_string(),
            }))
        }
        Sr25519Commands::Import {
            suri,
            password,
            storage,
        } => {
            use crate::schemes::sr25519::PrivateKey;

            let signer: Signer<Sr25519> = create_signer(storage);
            let private_key = PrivateKey::from_suri(&suri, password.as_deref())?;
            let public_key = signer.import_key(private_key)?;
            let address = signer.address(public_key);

            Ok(Sr25519Result::Import(KeyImportResult {
                public_key: hex::encode(public_key.to_bytes()),
                address: address.as_ss58().to_string(),
            }))
        }
        Sr25519Commands::Sign {
            public_key,
            data,
            storage,
            context,
        } => {
            let signer: Signer<Sr25519> = create_signer(storage);
            let public_key_bytes = hex::decode(&public_key)?;
            let mut public_key_arr = [0u8; 32];
            public_key_arr.copy_from_slice(&public_key_bytes);
            let public_key = PublicKey::from_bytes(public_key_arr);
            let data_bytes = hex::decode(&data)?;

            let signature = if let Some(ctx) = context {
                signer.sign_with_context(public_key, ctx.as_bytes(), &data_bytes)?
            } else {
                signer.sign(public_key, &data_bytes)?
            };

            Ok(Sr25519Result::Sign(SignResult {
                signature: hex::encode(signature.to_bytes()),
            }))
        }
        Sr25519Commands::Verify {
            public_key,
            data,
            signature,
            context,
        } => {
            let public_key_bytes = hex::decode(&public_key)?;
            let mut public_key_arr = [0u8; 32];
            public_key_arr.copy_from_slice(&public_key_bytes);
            let public_key = PublicKey::from_bytes(public_key_arr);
            let data_bytes = hex::decode(&data)?;
            let sig_bytes = hex::decode(&signature)?;
            let mut sig_arr = [0u8; 64];
            sig_arr.copy_from_slice(&sig_bytes);
            let signature = Signature::from_bytes(sig_arr);

            if let Some(ctx) = context {
                let signer: Signer<Sr25519> = Signer::memory();
                signer.verify_with_context(public_key, ctx.as_bytes(), &data_bytes, &signature)?;
            } else {
                <Sr25519 as SignatureScheme>::verify(&public_key, &data_bytes, &signature)?;
            }

            Ok(Sr25519Result::Verify(VerifyResult { valid: true }))
        }
        Sr25519Commands::Address { public_key } => {
            let public_key_bytes = hex::decode(&public_key)?;
            let mut public_key_arr = [0u8; 32];
            public_key_arr.copy_from_slice(&public_key_bytes);
            let address = crate::address::SubstrateAddress::new(public_key_arr)?;

            Ok(Sr25519Result::Address(AddressResult {
                address: address.as_ss58().to_string(),
            }))
        }
        Sr25519Commands::Keyring { command } => {
            let result = execute_keyring_command(command)?;
            match result {
                KeyringCommandResult::Keyring(r) => Ok(Sr25519Result::Keyring(r)),
                KeyringCommandResult::List(r) => Ok(Sr25519Result::KeyringList(r)),
            }
        }
        Sr25519Commands::List { storage } => {
            let signer: Signer<Sr25519> = create_signer(storage);
            let keys = signer.list_keys()?;

            let key_infos: Vec<KeyInfo> = keys
                .into_iter()
                .map(|key| {
                    let address = signer.address(key);
                    KeyInfo {
                        public_key: hex::encode(key.to_bytes()),
                        address: address.as_ss58().to_string(),
                    }
                })
                .collect();

            Ok(Sr25519Result::List(ListKeysResult { keys: key_infos }))
        }
    }
}

#[cfg(not(feature = "sr25519"))]
pub fn execute_sr25519_command(_command: Sr25519Commands) -> Result<Sr25519Result> {
    anyhow::bail!("sr25519 feature is not enabled. Rebuild with --features sr25519");
}

#[cfg(feature = "sr25519")]
enum KeyringCommandResult {
    Keyring(KeyringResult),
    List(KeyringListResult),
}

#[cfg(feature = "sr25519")]
fn execute_keyring_command(command: KeyringCommands) -> Result<KeyringCommandResult> {
    use crate::schemes::sr25519::Keyring;
    use schnorrkel::Keypair;

    match command {
        KeyringCommands::Create { path } => {
            Keyring::load(path.clone())?;
            Ok(KeyringCommandResult::Keyring(KeyringResult {
                message: format!("Created keyring at {}", path.display()),
                details: None,
            }))
        }
        KeyringCommands::Add {
            path,
            name,
            password,
        } => {
            let mut keyring = Keyring::load(path)?;
            let keypair = Keypair::generate();
            let passphrase = password.as_ref().map(|p| p.as_bytes());

            let keystore = keyring.add(&name, keypair.clone(), passphrase)?;
            let address = crate::address::SubstrateAddress::new(keypair.public.to_bytes())?;

            Ok(KeyringCommandResult::Keyring(KeyringResult {
                message: format!("Added key '{name}'"),
                details: Some(KeyringDetails {
                    name,
                    public_key: hex::encode(keypair.public.to_bytes()),
                    address: address.as_ss58().to_string(),
                    keystore_name: Some(keystore.meta.name),
                }),
            }))
        }
        KeyringCommands::Vanity {
            path,
            name,
            prefix,
            password,
        } => {
            let mut keyring = Keyring::load(path)?;
            let passphrase = password.as_ref().map(|p| p.as_bytes());

            let (keystore, keypair) = keyring.create_vanity(&name, &prefix, passphrase)?;
            let address = crate::address::SubstrateAddress::new(keypair.public.to_bytes())?;

            Ok(KeyringCommandResult::Keyring(KeyringResult {
                message: format!("Generated vanity key '{name}'"),
                details: Some(KeyringDetails {
                    name,
                    public_key: hex::encode(keypair.public.to_bytes()),
                    address: address.as_ss58().to_string(),
                    keystore_name: Some(keystore.meta.name),
                }),
            }))
        }
        KeyringCommands::List { path } => {
            let keyring = Keyring::load(path)?;
            let keystores = keyring.list();

            let keystore_infos: Vec<KeystoreInfo> = keystores
                .iter()
                .map(|ks| KeystoreInfo {
                    name: ks.meta.name.clone(),
                    public_key: ks.public_key().ok().map(hex::encode),
                    address: ks.address.clone(),
                    created: ks.meta.when_created.to_string(),
                })
                .collect();

            Ok(KeyringCommandResult::List(KeyringListResult {
                keystores: keystore_infos,
            }))
        }
    }
}

// Helper function to create signer with optional storage
fn create_signer<S: crate::traits::SignatureScheme>(storage: Option<PathBuf>) -> crate::Signer<S>
where
    S::PublicKey: serde::Serialize + serde::de::DeserializeOwned,
    S::PrivateKey: serde::Serialize + serde::de::DeserializeOwned,
{
    if let Some(path) = storage {
        crate::Signer::fs(path)
    } else {
        crate::Signer::memory()
    }
}
