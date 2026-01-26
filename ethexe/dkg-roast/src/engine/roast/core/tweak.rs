// This file is part of Gear.
//
// Copyright (C) 2025 Gear Technologies Inc.
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

use anyhow::{Result, anyhow};
use ethexe_common::{
    crypto::{
        DkgKeyPackage, DkgPublicKeyPackage,
        tweak::{tweak_pubkey, tweak_share},
    },
    k256::{Scalar, elliptic_curve::PrimeField},
};
use roast_secp256k1_evm::frost::{
    VerifyingKey,
    keys::{KeyPackage, PublicKeyPackage, SigningShare, VerifyingShare},
};
use std::collections::BTreeMap;

/// Parses a 32-byte scalar from raw bytes.
fn scalar_from_bytes(bytes: &[u8]) -> Result<Scalar> {
    let mut buf = [0u8; 32];
    if bytes.len() != buf.len() {
        return Err(anyhow!("Invalid scalar length"));
    }
    buf.copy_from_slice(bytes);
    Option::<Scalar>::from(Scalar::from_repr(buf.into())).ok_or_else(|| anyhow!("Malformed scalar"))
}

/// Applies a tweak to a signing share.
fn tweaked_signing_share(share: &SigningShare, tweak: Scalar) -> Result<SigningShare> {
    let scalar = scalar_from_bytes(&share.serialize())?;
    let tweaked = tweak_share(scalar, tweak);
    SigningShare::deserialize(&tweaked.to_bytes())
        .map_err(|err| anyhow!("Failed to deserialize tweaked signing share: {err}"))
}

/// Applies a tweak to a verifying share.
fn tweaked_verifying_share(share: &VerifyingShare, tweak: Scalar) -> Result<VerifyingShare> {
    let bytes = share.serialize()?;
    let compressed: [u8; 33] = bytes
        .as_slice()
        .try_into()
        .map_err(|_| anyhow!("Invalid verifying share length"))?;
    let tweaked = tweak_pubkey(&compressed, tweak)?;
    VerifyingShare::deserialize(&tweaked)
        .map_err(|err| anyhow!("Failed to deserialize tweaked verifying share: {err}"))
}

/// Applies a tweak to a verifying key.
fn tweaked_verifying_key(key: &VerifyingKey, tweak: Scalar) -> Result<VerifyingKey> {
    let bytes = key.serialize()?;
    let compressed: [u8; 33] = bytes
        .as_slice()
        .try_into()
        .map_err(|_| anyhow!("Invalid verifying key length"))?;
    let tweaked = tweak_pubkey(&compressed, tweak)?;
    VerifyingKey::deserialize(&tweaked)
        .map_err(|err| anyhow!("Failed to deserialize tweaked verifying key: {err}"))
}

/// Applies a tweak to a key package (signing + verifying shares + key).
pub(crate) fn tweak_key_package(
    key_package: &DkgKeyPackage,
    tweak: Scalar,
) -> Result<DkgKeyPackage> {
    let signing_share = tweaked_signing_share(key_package.signing_share(), tweak)?;
    let verifying_share = tweaked_verifying_share(key_package.verifying_share(), tweak)?;
    let verifying_key = tweaked_verifying_key(key_package.verifying_key(), tweak)?;

    Ok(KeyPackage::new(
        *key_package.identifier(),
        signing_share,
        verifying_share,
        verifying_key,
        *key_package.min_signers(),
    ))
}

/// Applies a tweak to the public key package (verifying shares + key).
pub(crate) fn tweak_public_key_package(
    public_key_package: &DkgPublicKeyPackage,
    tweak: Scalar,
) -> Result<DkgPublicKeyPackage> {
    let mut verifying_shares = BTreeMap::new();
    for (identifier, share) in public_key_package.verifying_shares() {
        verifying_shares.insert(*identifier, tweaked_verifying_share(share, tweak)?);
    }
    let verifying_key = tweaked_verifying_key(public_key_package.verifying_key(), tweak)?;
    Ok(PublicKeyPackage::new(verifying_shares, verifying_key))
}
