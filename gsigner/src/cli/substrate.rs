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

//! Shared substrate-style command plumbing for ed25519/sr25519.

use crate::{
    address::{SubstrateAddress, SubstrateCryptoScheme},
    cli::{
        commands::StorageLocationArgs,
        scheme::{AddressResult, KeyImportResult, SchemeFormatter, SchemeResult},
        util::decode_hex_array,
    },
    traits::{SeedableKey, SignatureScheme},
};
use anyhow::Result;
use hex;
use std::str::FromStr;

#[derive(Debug, Clone)]
pub enum SubstrateCommand {
    Clear {
        storage: StorageLocationArgs,
    },
    Generate {
        storage: StorageLocationArgs,
        show_secret: bool,
    },
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
    Clear(crate::cli::scheme::ClearResult),
    Generate(crate::cli::scheme::KeyGenerationResult),
    Import(crate::cli::scheme::KeyImportResult),
    Sign(crate::cli::scheme::SignResult),
    Verify(crate::cli::scheme::VerifyResult),
    Address(crate::cli::scheme::AddressResult),
}

pub type SubstrateSignFn<S> = fn(
    &crate::Signer<S>,
    <S as SignatureScheme>::PublicKey,
    &[u8],
    Option<String>,
) -> Result<<S as SignatureScheme>::Signature>;

pub type SubstrateVerifyFn<S> = fn(
    &<S as SignatureScheme>::PublicKey,
    &[u8],
    &<S as SignatureScheme>::Signature,
    Option<String>,
) -> Result<()>;

pub struct SubstrateDescriptor<S: crate::cli::storage::StorageScheme> {
    pub formatter: SchemeFormatter<S>,
    pub key_type_fn: fn() -> String,
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
        SubstrateResult::Clear(r) => SchemeResult::Clear(r),
        SubstrateResult::Generate(r) => SchemeResult::Generate(r),
        SubstrateResult::Import(r) => SchemeResult::Import(r),
        SubstrateResult::Sign(r) => SchemeResult::Sign(r),
        SubstrateResult::Verify(r) => SchemeResult::Verify(r),
        SubstrateResult::Address(r) => SchemeResult::Address(r),
    }
}

#[allow(clippy::too_many_arguments)]
pub fn make_substrate_descriptor<S>(
    formatter: SchemeFormatter<S>,
    key_type_fn: fn() -> String,
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
        formatter,
        key_type_fn,
        parse_public,
        parse_signature,
        sign_fn,
        verify_fn,
        signature_hex,
        import_private,
        network_scheme,
    }
}

pub fn substrate_formatter<S>(
    scheme_name: &'static str,
    key_type_fn: fn() -> String,
    public_fmt: fn(&S::PublicKey) -> String,
) -> SchemeFormatter<S>
where
    S: SignatureScheme<Address = SubstrateAddress>,
{
    SchemeFormatter {
        scheme_name,
        key_type_fn,
        public_fmt,
        address_fmt: crate::cli::util::substrate_address_display,
    }
}

pub fn execute_substrate_command<S>(
    desc: &SubstrateDescriptor<S>,
    command: SubstrateCommand,
) -> Result<SubstrateResult>
where
    S: crate::cli::storage::StorageScheme,
    S: SignatureScheme<Address = SubstrateAddress>,
    S::PrivateKey: SeedableKey + Clone,
{
    let formatter = &desc.formatter;
    match command {
        SubstrateCommand::Clear { storage } => {
            let result = crate::cli::storage::clear_keys_command::<S>(&storage)?;
            Ok(SubstrateResult::Clear(result))
        }
        SubstrateCommand::Generate {
            storage,
            show_secret,
        } => {
            let result =
                crate::cli::storage::generate_key_result::<S>(&storage, formatter, show_secret)?;
            Ok(SubstrateResult::Generate(result))
        }
        SubstrateCommand::Import {
            suri,
            seed,
            suri_password,
            storage,
            show_secret,
        } => crate::cli::storage::with_signer::<S, _, _>(&storage, |signer| {
            if suri_password.is_some() && suri.is_none() {
                anyhow::bail!("--password can only be used together with --suri");
            }

            let private_key = if let Some(seed_hex) = seed {
                let seed_value = crate::cli::storage::seed_from_hex::<S>(&seed_hex)?;
                S::PrivateKey::from_seed(seed_value)?
            } else {
                let suri = suri.expect("clap ensures either --suri or --seed is provided");
                (desc.import_private)(&suri, suri_password.as_deref())?
            };
            let public_key = signer.import_key(private_key.clone())?;
            let public_display = formatter.format_public(&public_key);
            let address = signer.address(public_key);

            Ok(SubstrateResult::Import(KeyImportResult {
                public_key: public_display,
                address: formatter.format_address(&address),
                scheme: formatter.scheme_name().to_string(),
                key_type: (desc.key_type_fn)(),
                secret: show_secret.then(|| hex::encode(private_key.seed().as_ref())),
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
                |signer, public, message| (desc.sign_fn)(signer, public, message, context.clone()),
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
                address: formatter.format_address(&address),
            }))
        }
    }
}

pub fn substrate_public_display(bytes: impl AsRef<[u8]>) -> String {
    hex::encode(bytes.as_ref())
}

#[cfg(feature = "ed25519")]
pub fn ed25519_public_display(key: &crate::schemes::ed25519::PublicKey) -> String {
    substrate_public_display(key.to_bytes())
}

#[cfg(feature = "sr25519")]
pub fn sr25519_public_display(key: &crate::schemes::sr25519::PublicKey) -> String {
    substrate_public_display(key.to_bytes())
}

#[cfg(feature = "ed25519")]
pub fn ed25519_key_type() -> String {
    crate::substrate::pair_key_type_string::<sp_core::ed25519::Pair>()
}

#[cfg(feature = "sr25519")]
pub fn sr25519_key_type() -> String {
    crate::substrate::pair_key_type_string::<sp_core::sr25519::Pair>()
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
    S::PrivateKey: SeedableKey,
    ParsePublic: Fn([u8; 32]) -> Public,
    SignerFn: Fn(&crate::Signer<S>, Public, &[u8]) -> Result<Sig>,
    SigHexFn: Fn(&Sig) -> String,
{
    crate::cli::storage::with_signer::<S, _, _>(storage, |signer| {
        let public_key = parse_public(decode_hex_array::<32>(public_key_hex, "public key")?);
        let message_bytes = crate::cli::util::prefixed_message(data_hex, prefix)?;
        let signature = signer_fn(&signer, public_key, &message_bytes)?;

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

pub fn parse_ss58_format(network: &Option<String>) -> Result<sp_core::crypto::Ss58AddressFormat> {
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
