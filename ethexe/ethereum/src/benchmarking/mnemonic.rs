// This file is part of Gear.
//
// Copyright (C) 2024-2026 Gear Technologies Inc.
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

use alloy::{
    primitives::Address,
    signers::local::{MnemonicBuilder, coins_bip39::English},
};
use anyhow::{Result, anyhow};
use gsigner::secp256k1::{PrivateKey, PublicKey, Signer};

/// Default Hardhat/Anvil mnemonic.
const MNEMONIC: &str = "test test test test test test test test test test test junk";

/// Derive a [`Signer`] (with one imported key) from the
/// standard derivation index `m/44'/60'/0'/0/{index}`.
///
/// Returns the signer together with the corresponding gsigner address.
pub fn derive_signer(index: u32) -> Result<(Signer, PublicKey, Address)> {
    // Derive the raw k256 key via alloy's BIP-32/BIP-39 MnemonicBuilder.
    let alloy_signer = MnemonicBuilder::<English>::default()
        .phrase(MNEMONIC)
        .index(index)
        .map_err(|e| anyhow!("bad derivation index {index}: {e}"))?
        .build()
        .map_err(|e| anyhow!("mnemonic derivation failed at index {index}: {e}"))?;

    // Extract the 32-byte secret and import it into a gsigner in-memory signer.
    let seed: [u8; 32] = alloy_signer.to_bytes().0;
    let private_key = PrivateKey::from_seed(seed)?;
    let signer = Signer::memory();
    let pubkey = signer.import(private_key)?;
    let address = pubkey.to_address();

    Ok((signer, pubkey, address.into()))
}
