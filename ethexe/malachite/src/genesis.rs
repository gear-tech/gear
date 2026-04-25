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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::PrivateKey;
    use std::io::Write;

    /// Generate a deterministic-ish secp256k1 keypair from a seed and
    /// return (address-as-hex-string, public_key-as-json-value,
    /// PublicKey).
    fn keypair_from_seed(seed: u8) -> (String, serde_json::Value, PublicKey) {
        let mut bytes = [0u8; 32];
        bytes[31] = seed;
        let priv_key = PrivateKey::from_slice(&bytes).expect("valid scalar");
        let pub_key = priv_key.public_key();
        let addr = Address::from_public_key(&pub_key);
        let addr_hex = format!("{addr}");
        let pub_json = serde_json::to_value(&pub_key).expect("PublicKey is serde");
        (addr_hex, pub_json, pub_key)
    }

    fn write_genesis_to_temp(json: &str) -> tempfile::NamedTempFile {
        let mut f = tempfile::NamedTempFile::new().expect("temp file");
        f.write_all(json.as_bytes()).expect("write");
        f
    }

    #[test]
    fn load_rejects_missing_file() {
        let path = std::path::PathBuf::from("/this/does/not/exist/genesis.json");
        let err = MalachiteGenesis::load(&path).unwrap_err();
        let msg = format!("{err:#}");
        assert!(msg.contains("reading Malachite genesis"), "got: {msg}");
    }

    #[test]
    fn load_rejects_empty_validator_set() {
        let f = write_genesis_to_temp(r#"{ "validators": [] }"#);
        let err = MalachiteGenesis::load(f.path()).unwrap_err();
        let msg = format!("{err:#}");
        assert!(msg.contains("empty validator set"), "got: {msg}");
    }

    #[test]
    fn load_accepts_consistent_validator_entry() {
        let (addr_hex, pub_json, _) = keypair_from_seed(1);
        let json = format!(
            r#"{{ "validators": [
                {{ "address": "{addr_hex}",
                   "public_key": {pub_json},
                   "voting_power": 7 }}
            ] }}"#
        );
        let f = write_genesis_to_temp(&json);
        let g = MalachiteGenesis::load(f.path()).expect("load");
        assert_eq!(g.validators.len(), 1);
        assert_eq!(g.validators[0].voting_power, 7);
    }

    #[test]
    fn load_rejects_address_pubkey_mismatch() {
        // Generate two keypairs; pair the address from one with the
        // pubkey from the other. The integrity check must reject.
        let (addr_a, _, _) = keypair_from_seed(1);
        let (_, pub_b_json, _) = keypair_from_seed(2);
        let json = format!(
            r#"{{ "validators": [
                {{ "address": "{addr_a}",
                   "public_key": {pub_b_json},
                   "voting_power": 1 }}
            ] }}"#
        );
        let f = write_genesis_to_temp(&json);
        let err = MalachiteGenesis::load(f.path()).unwrap_err();
        let msg = format!("{err:#}");
        assert!(
            msg.contains("does not match"),
            "expected address/pubkey-mismatch error, got: {msg}"
        );
    }

    #[test]
    fn voting_power_defaults_to_one() {
        let (addr_hex, pub_json, _) = keypair_from_seed(3);
        // Note: no `voting_power` field in JSON.
        let json = format!(
            r#"{{ "validators": [
                {{ "address": "{addr_hex}", "public_key": {pub_json} }}
            ] }}"#
        );
        let f = write_genesis_to_temp(&json);
        let g = MalachiteGenesis::load(f.path()).expect("load");
        assert_eq!(g.validators[0].voting_power, 1);
    }

    #[test]
    fn to_validator_set_preserves_entries() {
        let (addr1, pub1, _) = keypair_from_seed(11);
        let (addr2, pub2, _) = keypair_from_seed(12);
        let json = format!(
            r#"{{ "validators": [
                {{ "address": "{addr1}", "public_key": {pub1}, "voting_power": 3 }},
                {{ "address": "{addr2}", "public_key": {pub2}, "voting_power": 5 }}
            ] }}"#
        );
        let f = write_genesis_to_temp(&json);
        let g = MalachiteGenesis::load(f.path()).expect("load");
        let vs = g.to_validator_set();
        // Static-trait-bound check via use of the trait method.
        use malachitebft_core_types::ValidatorSet as _;
        assert_eq!(vs.count(), 2);
    }
}
