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

//! Scheme descriptors and result types shared between CLI handlers and display.

use anyhow::Result;
use serde::Serialize;

#[cfg(feature = "secp256k1")]
use crate::{schemes::secp256k1, substrate::pair_key_type_string, traits::SignatureScheme};

/// Result of key generation.
#[derive(Debug, Clone, Serialize)]
pub struct KeyGenerationResult {
    pub public_key: String,
    pub address: String,
    pub scheme: String,
    pub key_type: String,
    pub secret: Option<String>,
    pub name: Option<String>,
}

/// Result of key import.
#[derive(Debug, Clone, Serialize)]
pub struct KeyImportResult {
    pub public_key: String,
    pub address: String,
    pub scheme: String,
    pub key_type: String,
    pub secret: Option<String>,
    pub name: Option<String>,
}

/// Result of signing operation.
#[derive(Debug, Clone, Serialize)]
pub struct SignResult {
    pub signature: String,
}

/// Result of verification operation.
#[derive(Debug, Clone, Serialize)]
pub struct VerifyResult {
    pub valid: bool,
}

/// Result of address conversion.
#[derive(Debug, Clone, Serialize)]
pub struct AddressResult {
    pub address: String,
}

/// Result of recovering a public key from a signature.
#[derive(Debug, Clone, Serialize)]
pub struct RecoverResult {
    pub public_key: String,
    pub address: String,
}

/// Result of PeerId derivation.
#[derive(Debug, Clone, Serialize)]
pub struct PeerIdResult {
    pub peer_id: String,
}

/// Result of listing keys.
#[derive(Debug, Clone, Serialize)]
pub struct ListKeysResult {
    pub keys: Vec<KeyInfo>,
}

/// Result of clearing keys.
#[derive(Debug, Clone, Serialize)]
pub struct ClearResult {
    pub removed: usize,
}

/// Information about a key.
#[derive(Debug, Clone, Serialize)]
pub struct KeyInfo {
    pub public_key: String,
    pub address: String,
    pub scheme: String,
    pub key_type: String,
    pub secret: Option<String>,
    pub name: Option<String>,
}

/// Generic success message result.
#[derive(Debug, Clone, Serialize)]
pub struct MessageResult {
    pub message: String,
}

/// Scheme identifier used for results.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum Scheme {
    Secp256k1,
    Ed25519,
    Sr25519,
}

/// Result of command execution.
#[derive(Debug, Clone, Serialize)]
pub struct CommandResult {
    pub scheme: Scheme,
    pub result: SchemeResult,
}

/// Unified result variants across schemes.
#[derive(Debug, Clone, Serialize)]
pub enum SchemeResult {
    Clear(ClearResult),
    Generate(KeyGenerationResult),
    Import(KeyImportResult),
    Sign(SignResult),
    Verify(VerifyResult),
    Recover(RecoverResult),
    Address(AddressResult),
    PeerId(PeerIdResult),
    List(ListKeysResult),
    Message(MessageResult),
}

/// Unified command wrapper for top-level scheme operations.
pub enum SchemeCommand<KeyringCommand> {
    #[cfg(feature = "keyring")]
    Keyring { command: KeyringCommand },
    Verify {
        public_key: String,
        data: String,
        prefix: Option<String>,
        signature: String,
        context: Option<String>,
    },
    Address {
        public_key: String,
        network: Option<String>,
    },
    Recover {
        data: String,
        prefix: Option<String>,
        signature: String,
    },
    #[cfg(feature = "peer-id")]
    PeerId { public_key: String },
}

#[derive(Clone, Copy)]
pub struct SchemeFormatter<S: crate::traits::SignatureScheme> {
    pub scheme_name: &'static str,
    pub key_type_fn: fn() -> String,
    pub public_fmt: fn(&S::PublicKey) -> String,
    pub address_fmt: fn(&S::Address) -> String,
}

impl<S: crate::traits::SignatureScheme> SchemeFormatter<S> {
    pub fn scheme_name(&self) -> &'static str {
        self.scheme_name
    }

    pub fn key_type(&self) -> String {
        (self.key_type_fn)()
    }

    pub fn format_public(&self, public: &S::PublicKey) -> String {
        (self.public_fmt)(public)
    }

    pub fn format_address(&self, address: &S::Address) -> String {
        (self.address_fmt)(address)
    }
}

#[cfg(feature = "secp256k1")]
pub fn secp256k1_formatter() -> SchemeFormatter<secp256k1::Secp256k1> {
    SchemeFormatter {
        scheme_name: secp256k1::Secp256k1::scheme_name(),
        key_type_fn: secp256k1_key_type,
        public_fmt: secp256k1_public_display,
        address_fmt: secp256k1_address_display,
    }
}

#[cfg(feature = "secp256k1")]
fn secp256k1_key_type() -> String {
    pair_key_type_string::<sp_core::ecdsa::Pair>()
}

#[cfg(feature = "secp256k1")]
fn secp256k1_public_display(key: &secp256k1::PublicKey) -> String {
    key.to_hex()
}

#[cfg(feature = "secp256k1")]
fn secp256k1_address_display(address: &secp256k1::Address) -> String {
    format!("0x{}", address.to_hex())
}

pub type SchemeVerifyFn =
    fn(String, String, Option<String>, String, Option<String>) -> Result<SchemeResult>;
pub type SchemeAddressFn = fn(String, Option<String>) -> Result<SchemeResult>;
pub type SchemeRecoverFn = fn(String, Option<String>, String) -> Result<SchemeResult>;
#[cfg(feature = "peer-id")]
pub type SchemePeerIdFn = fn(String) -> Result<SchemeResult>;

/// Generic descriptor for a signing scheme.
pub struct SchemeDescriptor<KeyringCommand> {
    #[allow(dead_code)]
    pub name: &'static str,
    #[cfg(feature = "keyring")]
    pub handle_keyring: fn(KeyringCommand) -> Result<SchemeResult>,
    pub verify: SchemeVerifyFn,
    pub address: SchemeAddressFn,
    pub recover: Option<SchemeRecoverFn>,
    #[cfg(feature = "peer-id")]
    pub peer_id: Option<SchemePeerIdFn>,
}

pub fn execute_scheme_command<KeyringCommand>(
    descriptor: &SchemeDescriptor<KeyringCommand>,
    command: SchemeCommand<KeyringCommand>,
) -> Result<SchemeResult> {
    match command {
        #[cfg(feature = "keyring")]
        SchemeCommand::Keyring { command } => (descriptor.handle_keyring)(command),
        SchemeCommand::Verify {
            public_key,
            data,
            prefix,
            signature,
            context,
        } => (descriptor.verify)(public_key, data, prefix, signature, context),
        SchemeCommand::Address {
            public_key,
            network,
        } => (descriptor.address)(public_key, network),
        SchemeCommand::Recover {
            data,
            prefix,
            signature,
        } => {
            let recover_fn = descriptor
                .recover
                .ok_or_else(|| anyhow::anyhow!("Recovery is not supported for this scheme"))?;
            recover_fn(data, prefix, signature)
        }
        #[cfg(feature = "peer-id")]
        SchemeCommand::PeerId { public_key } => {
            let peer_id_fn = descriptor.peer_id.ok_or_else(|| {
                anyhow::anyhow!("PeerId derivation is not supported for this scheme")
            })?;
            peer_id_fn(public_key)
        }
    }
}
