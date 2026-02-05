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

//! Command handlers for gsigner CLI.
//!
//! These handlers execute the commands and return structured results.
//! They don't print directly, allowing integrators to customize output formatting.

#[cfg(feature = "keyring")]
use super::keyring_ops::{
    GenericKeyringOps, HexImporter, KeyringCommandHandler, KeyringOpsExt, PrivateKeyDisplay,
    PublicKeyExtractor, VanityMatcher, execute_keyring_command,
};
use super::{
    commands::*,
    keyring_ops::*,
    scheme::*,
    storage::*,
    util::{prefixed_message, strip_0x, validate_hex_len},
};
#[cfg(feature = "secp256k1")]
use alloy_primitives::utils::EIP191_PREFIX;
use anyhow::Result;
use secrecy::ExposeSecret;
use std::str::FromStr;

#[cfg(any(feature = "secp256k1", feature = "ed25519", feature = "sr25519"))]
use crate::scheme::CryptoScheme;

#[cfg(all(feature = "ed25519", feature = "peer-id"))]
use crate::cli::util::decode_hex_array;

#[cfg(feature = "secp256k1")]
mod secp_ops {
    use crate::{
        cli::{
            scheme::{AddressResult, RecoverResult, SchemeResult, VerifyResult},
            util::{prefixed_message, strip_0x, validate_hex_len},
        },
        scheme::CryptoScheme,
        schemes::secp256k1::{PublicKey, Secp256k1, Signature},
    };
    use anyhow::Result;

    pub fn verify(
        public_key: String,
        data: String,
        prefix: Option<String>,
        signature: String,
    ) -> Result<SchemeResult> {
        validate_hex_len(&public_key, 33, "public key")?;
        let public_key: PublicKey = public_key.parse()?;
        validate_hex_len(&signature, 65, "signature")?;
        let message_bytes = prefixed_message(&data, &prefix)?;
        let sig_bytes = hex::decode(strip_0x(&signature))?;
        let mut sig_arr = [0u8; 65];
        sig_arr.copy_from_slice(&sig_bytes);
        let signature = Signature::from_pre_eip155_bytes(sig_arr)
            .ok_or_else(|| anyhow::anyhow!("Invalid signature"))?;

        Secp256k1::verify(&public_key, &message_bytes, &signature)?;

        Ok(SchemeResult::Verify(VerifyResult { valid: true }))
    }

    pub fn recover(
        data: String,
        prefix: Option<String>,
        signature: String,
    ) -> Result<SchemeResult> {
        validate_hex_len(&signature, 65, "signature")?;
        let message_bytes = prefixed_message(&data, &prefix)?;
        let sig_bytes = hex::decode(strip_0x(&signature))?;
        let mut sig_arr = [0u8; 65];
        sig_arr.copy_from_slice(&sig_bytes);
        let signature = Signature::from_pre_eip155_bytes(sig_arr)
            .ok_or_else(|| anyhow::anyhow!("Invalid signature"))?;
        let public: PublicKey = signature.recover(&message_bytes)?;
        let address = public.to_address();

        Ok(SchemeResult::Recover(RecoverResult {
            public_key: Secp256k1::public_key_to_hex(&public),
            address: Secp256k1::address_to_string(&address),
        }))
    }

    pub fn address(public_key: String) -> Result<SchemeResult> {
        validate_hex_len(&public_key, 33, "public key")?;
        let public_key: PublicKey = public_key.parse()?;
        let address = public_key.to_address();

        Ok(SchemeResult::Address(AddressResult {
            address: Secp256k1::address_to_string(&address),
        }))
    }
}

#[cfg(feature = "secp256k1")]
use secp_ops::*;

#[cfg(any(feature = "ed25519", feature = "sr25519"))]
mod substrate_ops {
    use crate::{
        address::{SubstrateAddress, SubstrateCryptoScheme},
        cli::{
            commands::StorageLocationArgs,
            scheme::{AddressResult, KeyImportResult, SchemeResult},
            util::decode_hex_array,
        },
        scheme::CryptoScheme,
    };
    use anyhow::Result;
    use secrecy::ExposeSecret;

    #[derive(Debug, Clone)]
    pub enum SubstrateCommand {
        Import {
            suri: Option<String>,
            seed: Option<String>,
            suri_password: Option<String>,
            storage: StorageLocationArgs,
            show_secret: bool,
        },
        Sign {
            public_key: String,
            data: String,
            prefix: Option<String>,
            storage: StorageLocationArgs,
            context: Option<String>,
        },
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
    }

    #[derive(Debug, Clone)]
    pub enum SubstrateResult {
        Import(crate::cli::scheme::KeyImportResult),
        Sign(crate::cli::scheme::SignResult),
        Verify(crate::cli::scheme::VerifyResult),
        Address(crate::cli::scheme::AddressResult),
    }

    pub type SubstrateSignFn<S> = fn(
        &crate::Signer<S>,
        <S as CryptoScheme>::PublicKey,
        &[u8],
        Option<String>,
        Option<&str>,
    ) -> Result<<S as CryptoScheme>::Signature>;

    pub type SubstrateVerifyFn<S> = fn(
        &<S as CryptoScheme>::PublicKey,
        &[u8],
        &<S as CryptoScheme>::Signature,
        Option<String>,
    ) -> Result<()>;

    pub struct SubstrateDescriptor<S: crate::cli::storage::StorageScheme> {
        pub parse_public: fn([u8; 32]) -> S::PublicKey,
        pub parse_signature: fn([u8; 64]) -> S::Signature,
        pub sign_fn: SubstrateSignFn<S>,
        pub verify_fn: SubstrateVerifyFn<S>,
        pub signature_hex: fn(&S::Signature) -> String,
        pub import_private: fn(&str, Option<&str>) -> Result<S::PrivateKey>,
        pub network_scheme: SubstrateCryptoScheme,
    }

    pub fn substrate_result_to_scheme(res: SubstrateResult) -> SchemeResult {
        match res {
            SubstrateResult::Import(r) => SchemeResult::Import(r),
            SubstrateResult::Sign(r) => SchemeResult::Sign(r),
            SubstrateResult::Verify(r) => SchemeResult::Verify(r),
            SubstrateResult::Address(r) => SchemeResult::Address(r),
        }
    }

    pub fn make_substrate_descriptor<S>(
        parse_public: fn([u8; 32]) -> S::PublicKey,
        parse_signature: fn([u8; 64]) -> S::Signature,
        sign_fn: SubstrateSignFn<S>,
        verify_fn: SubstrateVerifyFn<S>,
        signature_hex: fn(&S::Signature) -> String,
        import_private: fn(&str, Option<&str>) -> Result<S::PrivateKey>,
        network_scheme: SubstrateCryptoScheme,
    ) -> SubstrateDescriptor<S>
    where
        S: crate::cli::storage::StorageScheme,
    {
        SubstrateDescriptor {
            parse_public,
            parse_signature,
            sign_fn,
            verify_fn,
            signature_hex,
            import_private,
            network_scheme,
        }
    }

    pub fn execute_substrate_command<S>(
        desc: &SubstrateDescriptor<S>,
        command: SubstrateCommand,
    ) -> Result<SubstrateResult>
    where
        S: crate::cli::storage::StorageScheme,
        S: CryptoScheme<Address = SubstrateAddress>,
    {
        match command {
            SubstrateCommand::Import {
                suri,
                seed,
                suri_password,
                storage,
                show_secret,
            } => crate::cli::storage::with_signer::<S, _, _>(&storage, |signer| {
                let password = storage
                    .key_password
                    .as_ref()
                    .map(|secret| secret.expose_secret().as_str());
                if suri_password.is_some() && suri.is_none() {
                    anyhow::bail!("--password can only be used together with --suri");
                }

                let private_key = if let Some(seed_hex) = seed {
                    let seed_value = crate::cli::storage::seed_from_hex::<S>(&seed_hex)?;
                    S::private_key_from_seed(seed_value)?
                } else {
                    let suri = suri.expect("clap ensures either --suri or --seed is provided");
                    (desc.import_private)(&suri, suri_password.as_deref())?
                };
                let public_key = if let Some(pwd) = password {
                    signer.import_encrypted(private_key.clone(), pwd)?
                } else {
                    signer.import(private_key.clone())?
                };
                let address = signer.address(public_key.clone());

                Ok(SubstrateResult::Import(KeyImportResult {
                    public_key: S::public_key_to_hex(&public_key),
                    address: S::address_to_string(&address),
                    scheme: S::NAME.to_string(),
                    secret: show_secret
                        .then(|| hex::encode(S::private_key_to_seed(&private_key).as_ref())),
                    name: None,
                }))
            }),
            SubstrateCommand::Sign {
                public_key,
                data,
                prefix,
                storage,
                context,
            } => {
                let result = substrate_sign_command::<S, _, _, _, _, _>(
                    &public_key,
                    &data,
                    &prefix,
                    &storage,
                    desc.parse_public,
                    |signer, public, message, password| {
                        (desc.sign_fn)(signer, public, message, context.clone(), password)
                    },
                    |signature| (desc.signature_hex)(signature),
                )?;
                Ok(SubstrateResult::Sign(result))
            }
            SubstrateCommand::Verify {
                public_key,
                data,
                prefix,
                signature,
                context,
            } => {
                let result = substrate_verify_command(
                    &public_key,
                    &data,
                    &prefix,
                    &signature,
                    desc.parse_public,
                    desc.parse_signature,
                    |public, message, sig| (desc.verify_fn)(public, message, sig, context.clone()),
                )?;
                Ok(SubstrateResult::Verify(result))
            }
            SubstrateCommand::Address {
                public_key,
                network,
            } => {
                let public_key_arr = decode_hex_array::<32>(&public_key, "public key")?;
                let format = parse_ss58_format(&network)?;
                let address =
                    SubstrateAddress::new_with_format(public_key_arr, desc.network_scheme, format)?;

                Ok(SubstrateResult::Address(AddressResult {
                    address: S::address_to_string(&address),
                }))
            }
        }
    }

    fn substrate_sign_command<S, Public, Sig, ParsePublic, SignerFn, SigHexFn>(
        public_key_hex: &str,
        data_hex: &str,
        prefix: &Option<String>,
        storage: &StorageLocationArgs,
        parse_public: ParsePublic,
        signer_fn: SignerFn,
        sig_hex_fn: SigHexFn,
    ) -> Result<crate::cli::scheme::SignResult>
    where
        S: crate::cli::storage::StorageScheme,
        ParsePublic: Fn([u8; 32]) -> Public,
        SignerFn: Fn(&crate::Signer<S>, Public, &[u8], Option<&str>) -> Result<Sig>,
        SigHexFn: Fn(&Sig) -> String,
    {
        crate::cli::storage::with_signer::<S, _, _>(storage, |signer| {
            let password = storage
                .key_password
                .as_ref()
                .map(|secret| secret.expose_secret().as_str());
            let public_key = parse_public(decode_hex_array::<32>(public_key_hex, "public key")?);
            let message_bytes = crate::cli::util::prefixed_message(data_hex, prefix)?;
            let signature = signer_fn(&signer, public_key, &message_bytes, password)?;

            Ok(crate::cli::scheme::SignResult {
                signature: sig_hex_fn(&signature),
            })
        })
    }

    fn substrate_verify_command<Public, Sig, ParsePublic, ParseSig, VerifyFn>(
        public_key_hex: &str,
        data_hex: &str,
        prefix: &Option<String>,
        signature_hex: &str,
        parse_public: ParsePublic,
        parse_signature: ParseSig,
        verify_fn: VerifyFn,
    ) -> Result<crate::cli::scheme::VerifyResult>
    where
        ParsePublic: Fn([u8; 32]) -> Public,
        ParseSig: Fn([u8; 64]) -> Sig,
        VerifyFn: Fn(&Public, &[u8], &Sig) -> Result<()>,
    {
        let public_key = parse_public(decode_hex_array::<32>(public_key_hex, "public key")?);
        let signature = parse_signature(decode_hex_array::<64>(signature_hex, "signature")?);
        let message_bytes = crate::cli::util::prefixed_message(data_hex, prefix)?;

        verify_fn(&public_key, &message_bytes, &signature)?;

        Ok(crate::cli::scheme::VerifyResult { valid: true })
    }

    pub fn parse_ss58_format(
        network: &Option<String>,
    ) -> Result<sp_core::crypto::Ss58AddressFormat> {
        use std::str::FromStr;

        if let Some(net) = network {
            if let Ok(prefix) = net.parse::<u16>() {
                return Ok(sp_core::crypto::Ss58AddressFormat::custom(prefix));
            }

            let reg = sp_core::crypto::Ss58AddressFormatRegistry::from_str(net)
                .map_err(|_| anyhow::anyhow!("Unknown network prefix '{net}'"))?;
            Ok(sp_core::crypto::Ss58AddressFormat::from(reg))
        } else {
            Ok(sp_core::crypto::Ss58AddressFormat::custom(
                SubstrateAddress::DEFAULT_PREFIX,
            ))
        }
    }
}

#[cfg(any(feature = "ed25519", feature = "sr25519"))]
use substrate_ops::*;

/// Execute a gsigner command.
pub fn execute_command(command: GSignerCommands) -> Result<CommandResult> {
    let (scheme, command) = command.into_scheme_and_subcommand();
    let handler = scheme_handlers()
        .into_iter()
        .find(|entry| entry.scheme == scheme)
        .ok_or_else(|| anyhow::anyhow!("handler for scheme {:?} is not available", scheme))?;

    let result = (handler.handler)(command)?;
    Ok(CommandResult { scheme, result })
}

struct SchemeHandlerEntry {
    scheme: Scheme,
    handler: fn(SchemeSubcommand) -> Result<SchemeResult>,
}

fn dispatch_subcommand(
    descriptor: &SchemeDescriptor<SchemeKeyringCommands>,
    command: SchemeSubcommand,
) -> Result<SchemeResult> {
    match command {
        #[cfg(feature = "keyring")]
        SchemeSubcommand::Keyring { command } => {
            execute_scheme_command(descriptor, SchemeCommand::Keyring { command })
        }
        SchemeSubcommand::Verify {
            public_key,
            data,
            prefix,
            signature,
            context,
        } => execute_scheme_command(
            descriptor,
            SchemeCommand::Verify {
                public_key,
                data,
                prefix,
                signature,
                context,
            },
        ),
        SchemeSubcommand::Address {
            public_key,
            network,
        } => execute_scheme_command(
            descriptor,
            SchemeCommand::Address {
                public_key,
                network,
            },
        ),
        SchemeSubcommand::Recover {
            data,
            prefix,
            signature,
        } => execute_scheme_command(
            descriptor,
            SchemeCommand::Recover {
                data,
                prefix,
                signature,
            },
        ),
        #[cfg(feature = "peer-id")]
        SchemeSubcommand::PeerId { public_key } => {
            execute_scheme_command(descriptor, SchemeCommand::PeerId { public_key })
        }
    }
}

#[allow(clippy::vec_init_then_push)]
fn scheme_handlers() -> Vec<SchemeHandlerEntry> {
    let mut entries = Vec::new();
    #[cfg(feature = "secp256k1")]
    entries.push(SchemeHandlerEntry {
        scheme: Scheme::Secp256k1,
        handler: |cmd| dispatch_subcommand(&secp256k1_scheme_descriptor(), cmd),
    });
    #[cfg(not(feature = "secp256k1"))]
    entries.push(SchemeHandlerEntry {
        scheme: Scheme::Secp256k1,
        handler: |_cmd| {
            anyhow::bail!("secp256k1 feature is not enabled. Rebuild with --features secp256k1")
        },
    });

    #[cfg(feature = "ed25519")]
    entries.push(SchemeHandlerEntry {
        scheme: Scheme::Ed25519,
        handler: |cmd| dispatch_subcommand(&ed25519_scheme_descriptor(), cmd),
    });
    #[cfg(not(feature = "ed25519"))]
    entries.push(SchemeHandlerEntry {
        scheme: Scheme::Ed25519,
        handler: |_cmd| {
            anyhow::bail!("ed25519 feature is not enabled. Rebuild with --features ed25519")
        },
    });

    #[cfg(feature = "sr25519")]
    entries.push(SchemeHandlerEntry {
        scheme: Scheme::Sr25519,
        handler: |cmd| dispatch_subcommand(&sr25519_scheme_descriptor(), cmd),
    });
    #[cfg(not(feature = "sr25519"))]
    entries.push(SchemeHandlerEntry {
        scheme: Scheme::Sr25519,
        handler: |_cmd| {
            anyhow::bail!("sr25519 feature is not enabled. Rebuild with --features sr25519")
        },
    });

    entries
}

#[cfg(feature = "secp256k1")]
fn secp256k1_scheme_descriptor() -> SchemeDescriptor<SchemeKeyringCommands> {
    SchemeDescriptor {
        #[cfg(feature = "keyring")]
        handle_keyring: secp256k1_handle_keyring,
        verify: |public_key, data, prefix, signature, _| {
            let effective_prefix = prefix.or_else(|| Some(EIP191_PREFIX.to_string()));
            verify(public_key, data, effective_prefix, signature)
        },
        address: |public_key, _network| address(public_key),
        recover: Some(|data, prefix, signature| {
            let effective_prefix = prefix.or_else(|| Some(EIP191_PREFIX.to_string()));
            recover(data, effective_prefix, signature)
        }),
        #[cfg(feature = "peer-id")]
        peer_id: Some(secp256k1_peer_id),
    }
}

#[cfg(all(feature = "secp256k1", feature = "keyring"))]
fn secp256k1_handle_keyring(command: SchemeKeyringCommands) -> Result<SchemeResult> {
    execute_keyring_command::<Secp256k1KeyringOps>(command)
}

#[cfg(all(feature = "secp256k1", feature = "peer-id"))]
fn secp256k1_peer_id(public_key: String) -> Result<SchemeResult> {
    use crate::schemes::secp256k1::PublicKey;

    validate_hex_len(&public_key, 33, "public key")?;
    let public_key: PublicKey = public_key.parse()?;
    let peer_id = crate::peer_id::peer_id_from_secp256k1(&public_key)?;

    Ok(SchemeResult::PeerId(PeerIdResult {
        peer_id: peer_id.to_string(),
    }))
}

#[cfg(feature = "ed25519")]
fn ed25519_scheme_descriptor() -> SchemeDescriptor<SchemeKeyringCommands> {
    SchemeDescriptor {
        #[cfg(feature = "keyring")]
        handle_keyring: ed25519_handle_keyring,
        verify: ed25519_verify,
        address: ed25519_address,
        recover: None,
        #[cfg(feature = "peer-id")]
        peer_id: Some(ed25519_peer_id),
    }
}

#[cfg(all(feature = "ed25519", feature = "keyring"))]
fn ed25519_handle_keyring(command: SchemeKeyringCommands) -> Result<SchemeResult> {
    execute_keyring_command::<Ed25519KeyringOps>(command)
}

#[cfg(feature = "ed25519")]
fn ed25519_verify(
    public_key: String,
    data: String,
    prefix: Option<String>,
    signature: String,
    _context: Option<String>,
) -> Result<SchemeResult> {
    let res = execute_substrate_command(
        &ed25519_descriptor(),
        SubstrateCommand::Verify {
            public_key,
            data,
            prefix,
            signature,
            context: None,
        },
    )?;
    Ok(substrate_result_to_scheme(res))
}

#[cfg(feature = "ed25519")]
fn ed25519_address(public_key: String, network: Option<String>) -> Result<SchemeResult> {
    let res = execute_substrate_command(
        &ed25519_descriptor(),
        SubstrateCommand::Address {
            public_key,
            network,
        },
    )?;
    Ok(substrate_result_to_scheme(res))
}

#[cfg(all(feature = "ed25519", feature = "peer-id"))]
fn ed25519_peer_id(public_key: String) -> Result<SchemeResult> {
    let public_key_bytes = decode_hex_array::<32>(strip_0x(&public_key), "public key")?;
    let public_key = crate::schemes::ed25519::PublicKey::from_bytes(public_key_bytes);
    let peer_id = crate::peer_id::peer_id_from_ed25519(&public_key)?;

    Ok(SchemeResult::PeerId(PeerIdResult {
        peer_id: peer_id.to_string(),
    }))
}

#[cfg(feature = "sr25519")]
fn sr25519_scheme_descriptor() -> SchemeDescriptor<SchemeKeyringCommands> {
    SchemeDescriptor {
        #[cfg(feature = "keyring")]
        handle_keyring: sr25519_handle_keyring,
        verify: sr25519_verify,
        address: sr25519_address,
        recover: None,
        #[cfg(feature = "peer-id")]
        peer_id: None,
    }
}

#[cfg(all(feature = "sr25519", feature = "keyring"))]
fn sr25519_handle_keyring(command: SchemeKeyringCommands) -> Result<SchemeResult> {
    execute_keyring_command::<Sr25519KeyringOps>(command)
}

#[cfg(feature = "sr25519")]
fn sr25519_verify(
    public_key: String,
    data: String,
    prefix: Option<String>,
    signature: String,
    context: Option<String>,
) -> Result<SchemeResult> {
    let res = execute_substrate_command(
        &sr25519_descriptor(),
        SubstrateCommand::Verify {
            public_key,
            data,
            prefix,
            signature,
            context: Some(context.unwrap_or_default()),
        },
    )?;
    Ok(substrate_result_to_scheme(res))
}

#[cfg(feature = "sr25519")]
fn sr25519_address(public_key: String, network: Option<String>) -> Result<SchemeResult> {
    let res = execute_substrate_command(
        &sr25519_descriptor(),
        SubstrateCommand::Address {
            public_key,
            network,
        },
    )?;
    Ok(substrate_result_to_scheme(res))
}

#[cfg(feature = "ed25519")]
fn ed25519_descriptor() -> SubstrateDescriptor<crate::schemes::ed25519::Ed25519> {
    make_substrate_descriptor(
        crate::schemes::ed25519::PublicKey::from_bytes,
        crate::schemes::ed25519::Signature::from_bytes,
        |signer, public, message, _context, password| {
            if let Some(pwd) = password {
                Ok(signer.sign_encrypted(public, message, pwd)?)
            } else {
                Ok(signer.sign(public, message)?)
            }
        },
        |public, message, signature, _context| {
            Ok(crate::schemes::ed25519::Ed25519::verify(
                public, message, signature,
            )?)
        },
        |signature| hex::encode(signature.to_bytes()),
        |suri, password| {
            Ok(crate::schemes::ed25519::PrivateKey::from_suri(
                suri, password,
            )?)
        },
        crate::address::SubstrateCryptoScheme::Ed25519,
    )
}

#[cfg(feature = "sr25519")]
fn sr25519_descriptor() -> SubstrateDescriptor<crate::schemes::sr25519::Sr25519> {
    use crate::schemes::sr25519::Sr25519SignerExt;

    make_substrate_descriptor(
        crate::schemes::sr25519::PublicKey::from_bytes,
        crate::schemes::sr25519::Signature::from_bytes,
        |signer, public, message, context, password| {
            let ctx = sr25519_context(&context)?;
            signer.sign_with_context(public, ctx.as_bytes(), message, password)
        },
        |public, message, signature, context| {
            let ctx = sr25519_context(&context)?;
            let signer: crate::Signer<crate::schemes::sr25519::Sr25519> = crate::Signer::memory();
            signer.verify_with_context(*public, ctx.as_bytes(), message, signature)
        },
        |signature| hex::encode(signature.to_bytes()),
        |suri, password| {
            Ok(crate::schemes::sr25519::PrivateKey::from_suri(
                suri, password,
            )?)
        },
        crate::address::SubstrateCryptoScheme::Sr25519,
    )
}

#[cfg(feature = "sr25519")]
fn sr25519_context(context: &Option<String>) -> Result<&str> {
    context
        .as_deref()
        .filter(|c| !c.is_empty())
        .ok_or_else(|| anyhow::anyhow!("sr25519 requires a non-empty signing context (--context)"))
}

#[cfg(all(feature = "secp256k1", feature = "keyring"))]
struct Secp256k1Ext;

#[cfg(all(feature = "secp256k1", feature = "keyring"))]
impl PrivateKeyDisplay for Secp256k1Ext {
    type PrivateKey = crate::schemes::secp256k1::PrivateKey;

    fn display_secret(private_key: &Self::PrivateKey) -> String {
        private_key.to_string()
    }
}

#[cfg(all(feature = "secp256k1", feature = "keyring"))]
impl VanityMatcher for Secp256k1Ext {
    type PrivateKey = crate::schemes::secp256k1::PrivateKey;
    type Address = crate::schemes::secp256k1::Address;

    fn normalize_prefix(prefix: &str) -> Result<String> {
        let normalized = prefix.trim();
        let normalized = normalized
            .strip_prefix("0x")
            .unwrap_or(normalized)
            .to_ascii_lowercase();
        if !normalized.chars().all(|c| c.is_ascii_hexdigit()) {
            anyhow::bail!("Prefix must be hexadecimal");
        }
        Ok(normalized)
    }

    fn matches_vanity(private_key: &Self::PrivateKey, prefix: &str) -> Result<bool> {
        let address_hex = private_key.public_key().to_address().to_hex();
        Ok(address_hex.starts_with(prefix))
    }
}

#[cfg(all(feature = "secp256k1", feature = "keyring"))]
impl HexImporter for Secp256k1Ext {
    type PrivateKey = crate::schemes::secp256k1::PrivateKey;

    fn import_hex(hex: &str) -> Result<Self::PrivateKey> {
        use crate::keyring::KeyCodec;
        crate::schemes::secp256k1::keyring::Secp256k1Codec::decode_private(hex)
    }
}

#[cfg(all(feature = "secp256k1", feature = "keyring"))]
impl PublicKeyExtractor<crate::schemes::secp256k1::keyring::Secp256k1Codec> for Secp256k1Ext {
    fn extract_public_key(
        keystore: &crate::keyring::SubstrateKeystore<
            crate::schemes::secp256k1::keyring::Secp256k1Codec,
        >,
    ) -> String {
        // Use CryptoScheme format (no 0x prefix) for CLI output consistency
        keystore
            .public_key()
            .map(|pk| crate::schemes::secp256k1::Secp256k1::public_key_to_hex(&pk))
            .unwrap_or_else(|_| "<unknown>".to_string())
    }
}

#[cfg(all(feature = "secp256k1", feature = "keyring"))]
impl KeyringOpsExt<crate::schemes::secp256k1::keyring::Secp256k1Codec> for Secp256k1Ext {
    const NAMESPACE: &'static str = crate::keyring::NAMESPACE_SECP;
}

#[cfg(all(feature = "secp256k1", feature = "keyring"))]
type Secp256k1KeyringOps =
    GenericKeyringOps<crate::schemes::secp256k1::keyring::Secp256k1Codec, Secp256k1Ext>;

#[cfg(all(feature = "secp256k1", feature = "keyring"))]
impl KeyringCommandHandler for Secp256k1KeyringOps {
    type Scheme = crate::schemes::secp256k1::Secp256k1;

    fn handle_sign(
        storage: &StorageLocationArgs,
        public_key: &str,
        data: &str,
        prefix: &Option<String>,
        _context: &Option<String>,
        contract: &Option<String>,
    ) -> Result<SchemeResult> {
        use crate::schemes::secp256k1::{PublicKey, Secp256k1SignerExt};

        with_signer::<Self::Scheme, _, _>(storage, |signer| {
            validate_hex_len(public_key, 33, "public key")?;
            let public_key: PublicKey = public_key.parse()?;
            let effective_prefix = prefix.clone().or_else(|| Some(EIP191_PREFIX.to_string()));
            let message_bytes = prefixed_message(data, &effective_prefix)?;
            let password = storage
                .key_password
                .as_ref()
                .map(|secret| secret.expose_secret().as_str());

            let signature = if let Some(contract_addr) = contract {
                let contract_bytes = hex::decode(strip_0x(contract_addr))?;
                if contract_bytes.len() != 20 {
                    anyhow::bail!("Contract address must be 20 bytes (40 hex characters)");
                }
                let mut contract_array = [0u8; 20];
                contract_array.copy_from_slice(&contract_bytes);
                let contract_address = crate::Address(contract_array);
                let signature = signer.sign_for_contract(
                    contract_address,
                    public_key,
                    &message_bytes,
                    password,
                )?;
                hex::encode(signature.into_pre_eip155_bytes())
            } else {
                let signature = if let Some(pwd) = password {
                    signer.sign_encrypted(public_key, &message_bytes, pwd)?
                } else {
                    signer.sign(public_key, &message_bytes)?
                };
                hex::encode(signature.into_pre_eip155_bytes())
            };

            Ok(SchemeResult::Sign(SignResult { signature }))
        })
    }

    fn handle_show(
        storage: &StorageLocationArgs,
        key: &str,
        show_secret: bool,
    ) -> Result<SchemeResult> {
        use crate::schemes::secp256k1::{Address, PublicKey};

        with_signer::<Self::Scheme, _, _>(storage, |signer| {
            let password = storage
                .key_password
                .as_ref()
                .map(|secret| secret.expose_secret().as_str());
            let public_key = if let Ok(pk) = PublicKey::from_str(key) {
                pk
            } else {
                let address = Address::from_str(key)
                    .map_err(|_| anyhow::anyhow!("Invalid public key or address '{key}'"))?;
                signer
                    .get_key_by_address(address)?
                    .ok_or_else(|| anyhow::anyhow!("No key found for address '{key}'"))?
            };

            let info = key_info_from_public(&signer, public_key, show_secret, password)?;
            Ok(SchemeResult::List(ListKeysResult { keys: vec![info] }))
        })
    }

    fn handle_import(
        storage: StorageLocationArgs,
        suri: Option<String>,
        seed: Option<String>,
        private_key: Option<String>,
        suri_password: Option<String>,
        name: Option<String>,
        show_secret: bool,
    ) -> Result<SchemeResult> {
        if seed.is_some() {
            anyhow::bail!("--seed is not supported for secp256k1 import");
        }
        let name =
            name.ok_or_else(|| anyhow::anyhow!("--name is required for secp256k1 import"))?;
        if suri_password.is_some() && suri.is_none() {
            anyhow::bail!("--suri_password can only be used together with --suri");
        }
        keyring_import::<Secp256k1KeyringOps, Self::Scheme, _>(
            storage,
            name.clone(),
            show_secret,
            false,
            |keyring, key_password| {
                if let Some(hex_key) = &private_key {
                    return Secp256k1KeyringOps::add_hex(keyring, &name, hex_key, key_password);
                }
                if let Some(suri) = &suri {
                    return Secp256k1KeyringOps::import_suri(
                        keyring,
                        &name,
                        suri,
                        suri_password.as_deref(),
                        key_password,
                    );
                }
                anyhow::bail!("either --private-key or --suri must be provided");
            },
        )
    }
}

#[cfg(all(feature = "ed25519", feature = "keyring"))]
struct Ed25519Ext;

#[cfg(all(feature = "ed25519", feature = "keyring"))]
impl PrivateKeyDisplay for Ed25519Ext {
    type PrivateKey = crate::schemes::ed25519::PrivateKey;

    fn display_secret(private_key: &Self::PrivateKey) -> String {
        hex::encode(private_key.to_bytes())
    }
}

#[cfg(all(feature = "ed25519", feature = "keyring"))]
impl VanityMatcher for Ed25519Ext {
    type PrivateKey = crate::schemes::ed25519::PrivateKey;
    type Address = crate::address::SubstrateAddress;

    fn normalize_prefix(prefix: &str) -> Result<String> {
        Ok(prefix.to_string())
    }

    fn matches_vanity(private_key: &Self::PrivateKey, prefix: &str) -> Result<bool> {
        let address = private_key.public_key().to_address()?;
        Ok(address.as_ss58().starts_with(prefix))
    }
}

#[cfg(all(feature = "ed25519", feature = "keyring"))]
impl HexImporter for Ed25519Ext {
    type PrivateKey = crate::schemes::ed25519::PrivateKey;

    fn import_hex(hex: &str) -> Result<Self::PrivateKey> {
        use crate::keyring::KeyCodec;
        crate::schemes::ed25519::keyring::Ed25519Codec::decode_private(hex)
    }
}

#[cfg(all(feature = "ed25519", feature = "keyring"))]
impl PublicKeyExtractor<crate::schemes::ed25519::keyring::Ed25519Codec> for Ed25519Ext {}

#[cfg(all(feature = "ed25519", feature = "keyring"))]
impl KeyringOpsExt<crate::schemes::ed25519::keyring::Ed25519Codec> for Ed25519Ext {
    const NAMESPACE: &'static str = crate::keyring::NAMESPACE_ED;
}

#[cfg(all(feature = "ed25519", feature = "keyring"))]
type Ed25519KeyringOps =
    GenericKeyringOps<crate::schemes::ed25519::keyring::Ed25519Codec, Ed25519Ext>;

#[cfg(all(feature = "ed25519", feature = "keyring"))]
impl KeyringCommandHandler for Ed25519KeyringOps {
    type Scheme = crate::schemes::ed25519::Ed25519;

    fn handle_sign(
        storage: &StorageLocationArgs,
        public_key: &str,
        data: &str,
        prefix: &Option<String>,
        _context: &Option<String>,
        _contract: &Option<String>,
    ) -> Result<SchemeResult> {
        let res = execute_substrate_command(
            &ed25519_descriptor(),
            SubstrateCommand::Sign {
                public_key: public_key.to_string(),
                data: data.to_string(),
                prefix: prefix.clone(),
                storage: storage.clone(),
                context: None,
            },
        )?;
        Ok(substrate_result_to_scheme(res))
    }

    fn handle_import(
        storage: StorageLocationArgs,
        suri: Option<String>,
        seed: Option<String>,
        private_key: Option<String>,
        suri_password: Option<String>,
        name: Option<String>,
        show_secret: bool,
    ) -> Result<SchemeResult> {
        if private_key.is_some() {
            anyhow::bail!("--private-key is not supported for ed25519");
        }
        if let Some(name) = name {
            if suri_password.is_some() && suri.is_none() {
                anyhow::bail!("--password can only be used together with --suri");
            }
            return keyring_import::<Ed25519KeyringOps, Self::Scheme, _>(
                storage,
                name.clone(),
                show_secret,
                true,
                |keyring, key_password| {
                    if let Some(seed_hex) = &seed {
                        return Ed25519KeyringOps::add_hex(keyring, &name, seed_hex, key_password);
                    }
                    if let Some(suri) = &suri {
                        return Ed25519KeyringOps::import_suri(
                            keyring,
                            &name,
                            suri,
                            suri_password.as_deref(),
                            key_password,
                        );
                    }
                    anyhow::bail!("either --seed or --suri must be provided");
                },
            );
        }
        // Fallback to substrate command for unnamed import
        let res = execute_substrate_command(
            &ed25519_descriptor(),
            SubstrateCommand::Import {
                suri,
                seed,
                suri_password,
                storage,
                show_secret,
            },
        )?;
        Ok(substrate_result_to_scheme(res))
    }
}

#[cfg(all(feature = "sr25519", feature = "keyring"))]
struct Sr25519Ext;

#[cfg(all(feature = "sr25519", feature = "keyring"))]
impl PrivateKeyDisplay for Sr25519Ext {
    type PrivateKey = crate::schemes::sr25519::PrivateKey;

    fn display_secret(private_key: &Self::PrivateKey) -> String {
        hex::encode(private_key.to_bytes())
    }
}

#[cfg(all(feature = "sr25519", feature = "keyring"))]
impl VanityMatcher for Sr25519Ext {
    type PrivateKey = crate::schemes::sr25519::PrivateKey;
    type Address = crate::address::SubstrateAddress;

    fn normalize_prefix(prefix: &str) -> Result<String> {
        Ok(prefix.to_string())
    }

    fn matches_vanity(private_key: &Self::PrivateKey, prefix: &str) -> Result<bool> {
        let address = private_key.public_key().to_address()?;
        Ok(address.as_ss58().starts_with(prefix))
    }
}

#[cfg(all(feature = "sr25519", feature = "keyring"))]
impl HexImporter for Sr25519Ext {
    type PrivateKey = crate::schemes::sr25519::PrivateKey;

    fn import_hex(hex: &str) -> Result<Self::PrivateKey> {
        use schnorrkel::KEYPAIR_LENGTH;
        let bytes = hex::decode(strip_0x(hex))?;
        if bytes.len() == 32 {
            let mut seed = [0u8; 32];
            seed.copy_from_slice(&bytes);
            crate::schemes::sr25519::PrivateKey::from_seed(seed)
                .map_err(|e| anyhow::anyhow!("{}", e))
        } else if bytes.len() == KEYPAIR_LENGTH {
            let keypair = schnorrkel::Keypair::from_half_ed25519_bytes(&bytes)
                .map_err(|e| anyhow::anyhow!("Invalid keypair: {:?}", e))?;
            Ok(crate::schemes::sr25519::PrivateKey::from_keypair(keypair))
        } else {
            anyhow::bail!("Seed must be 32 bytes (64 hex) or 96 bytes (192 hex)");
        }
    }
}

#[cfg(all(feature = "sr25519", feature = "keyring"))]
impl PublicKeyExtractor<crate::schemes::sr25519::keyring::Sr25519Codec> for Sr25519Ext {
    fn extract_public_key(
        keystore: &crate::keyring::SubstrateKeystore<
            crate::schemes::sr25519::keyring::Sr25519Codec,
        >,
    ) -> String {
        keystore
            .public_key()
            .map(|pk| hex::encode(pk.to_bytes()))
            .unwrap_or_else(|_| "<unknown>".to_string())
    }
}

#[cfg(all(feature = "sr25519", feature = "keyring"))]
impl KeyringOpsExt<crate::schemes::sr25519::keyring::Sr25519Codec> for Sr25519Ext {
    const NAMESPACE: &'static str = crate::keyring::NAMESPACE_SR;
}

#[cfg(all(feature = "sr25519", feature = "keyring"))]
type Sr25519KeyringOps =
    GenericKeyringOps<crate::schemes::sr25519::keyring::Sr25519Codec, Sr25519Ext>;

#[cfg(all(feature = "sr25519", feature = "keyring"))]
impl KeyringCommandHandler for Sr25519KeyringOps {
    type Scheme = crate::schemes::sr25519::Sr25519;

    fn handle_sign(
        storage: &StorageLocationArgs,
        public_key: &str,
        data: &str,
        prefix: &Option<String>,
        context: &Option<String>,
        _contract: &Option<String>,
    ) -> Result<SchemeResult> {
        let res = execute_substrate_command(
            &sr25519_descriptor(),
            SubstrateCommand::Sign {
                public_key: public_key.to_string(),
                data: data.to_string(),
                prefix: prefix.clone(),
                storage: storage.clone(),
                context: context.clone(),
            },
        )?;
        Ok(substrate_result_to_scheme(res))
    }

    fn handle_import(
        storage: StorageLocationArgs,
        suri: Option<String>,
        seed: Option<String>,
        private_key: Option<String>,
        suri_password: Option<String>,
        name: Option<String>,
        show_secret: bool,
    ) -> Result<SchemeResult> {
        if private_key.is_some() {
            anyhow::bail!("--private-key is not supported for sr25519");
        }
        if let Some(name) = name {
            if suri_password.is_some() && suri.is_none() {
                anyhow::bail!("--password can only be used together with --suri");
            }
            return keyring_import::<Sr25519KeyringOps, Self::Scheme, _>(
                storage,
                name.clone(),
                show_secret,
                true,
                |keyring, key_password| {
                    if let Some(seed_hex) = &seed {
                        return Sr25519KeyringOps::add_hex(keyring, &name, seed_hex, key_password);
                    }
                    if let Some(suri) = &suri {
                        return Sr25519KeyringOps::import_suri(
                            keyring,
                            &name,
                            suri,
                            suri_password.as_deref(),
                            key_password,
                        );
                    }
                    anyhow::bail!("either --seed or --suri must be provided");
                },
            );
        }
        // Fallback to substrate command for unnamed import
        let res = execute_substrate_command(
            &sr25519_descriptor(),
            SubstrateCommand::Import {
                suri,
                seed,
                suri_password,
                storage,
                show_secret,
            },
        )?;
        Ok(substrate_result_to_scheme(res))
    }
}
