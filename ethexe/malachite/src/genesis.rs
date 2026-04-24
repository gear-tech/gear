// This file is part of Gear.
//
// Copyright (C) 2026 Gear Technologies Inc.
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

//! Genesis config for Malachite: the address → public-key mapping
//! used to bootstrap [`ValidatorSet`].
//!
//! The file lives at `home_dir/genesis.json` and has the shape:
//! ```json
//! {
//!   "validators": [
//!     {
//!       "address":      "0xA1b2c3...",
//!       "public_key":   { "type": "tendermint/PubKeySecp256k1",
//!                         "value": "<base64 of 33-byte compressed point>" },
//!       "voting_power": 1
//!     },
//!     ...
//!   ]
//! }
//! ```
//!
//! Addresses are the 20-byte ethexe `gsigner::secp256k1::Address`
//! type (hex with `0x` prefix). Public keys use the serde format the
//! upstream `malachitebft-signing-ecdsa` crate already emits — this
//! way we can round-trip through `serde_json` without custom helpers
//! and the key format is explicit about the curve used.
//!
//! Each validator entry is consistency-checked at load time: the
//! address must equal `keccak256(uncompressed_pubkey)[12..]` of the
//! supplied public key. Inconsistent entries are rejected early,
//! before any votes are cast against a wrong identity.

use std::{fs, path::Path};

use anyhow::{Context as _, Result, anyhow};
use malachitebft_core_types::VotingPower;
use serde::{Deserialize, Serialize};

use crate::context::{Address, PublicKey, Validator, ValidatorSet};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenesisValidator {
    pub address: Address,
    pub public_key: PublicKey,
    #[serde(default = "default_voting_power")]
    pub voting_power: VotingPower,
}

fn default_voting_power() -> VotingPower {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MalachiteGenesis {
    pub validators: Vec<GenesisValidator>,
}

impl MalachiteGenesis {
    /// Read + parse the genesis file from disk.
    pub fn load(path: &Path) -> Result<Self> {
        let raw = fs::read_to_string(path)
            .with_context(|| format!("reading Malachite genesis from {}", path.display()))?;
        let parsed: Self = serde_json::from_str(&raw)
            .with_context(|| format!("parsing Malachite genesis from {}", path.display()))?;
        if parsed.validators.is_empty() {
            return Err(anyhow!(
                "Malachite genesis at {} contains an empty validator set",
                path.display()
            ));
        }
        for (i, v) in parsed.validators.iter().enumerate() {
            let derived = Address::from_public_key(&v.public_key);
            if derived != v.address {
                return Err(anyhow!(
                    "Malachite genesis validator #{i} address {} does not match \
                     address derived from its public key ({derived})",
                    v.address
                ));
            }
        }
        Ok(parsed)
    }

    /// Materialize into a Malachite [`ValidatorSet`]. Ordering is
    /// stable across nodes (by address), which is what BFT wants.
    pub fn to_validator_set(&self) -> ValidatorSet {
        ValidatorSet::new(self.validators.iter().map(|v| {
            Validator::with_address(
                v.address,
                v.public_key.clone(),
                v.voting_power,
            )
        }))
    }
}
