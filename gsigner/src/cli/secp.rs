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

//! Secp256k1-specific CLI helpers.

use crate::{
    cli::{
        scheme::{AddressResult, RecoverResult, SchemeResult, VerifyResult, secp256k1_formatter},
        util::{prefixed_message, strip_0x, validate_hex_len},
    },
    schemes::secp256k1::{PublicKey, Secp256k1, Signature},
    traits::SignatureScheme,
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

    <Secp256k1 as SignatureScheme>::verify(&public_key, &message_bytes, &signature)?;

    Ok(SchemeResult::Verify(VerifyResult { valid: true }))
}

pub fn recover(data: String, prefix: Option<String>, signature: String) -> Result<SchemeResult> {
    let formatter = secp256k1_formatter();
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
        public_key: formatter.format_public(&public),
        address: formatter.format_address(&address),
    }))
}

pub fn address(public_key: String) -> Result<SchemeResult> {
    let formatter = secp256k1_formatter();
    validate_hex_len(&public_key, 33, "public key")?;
    let public_key: PublicKey = public_key.parse()?;
    let address = public_key.to_address();

    Ok(SchemeResult::Address(AddressResult {
        address: formatter.format_address(&address),
    }))
}
