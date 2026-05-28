// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Parameters controlling the Malachite BFT consensus service.
//!
//! Kept in its own file (mirroring [`super::network`]) because the set
//! of user-facing knobs is expected to grow considerably — peer
//! discovery, persistent peers, timeouts, gas budget, etc.

use super::MergeParams;
use anyhow::{Context, Result};
use clap::Parser;
use ethexe_malachite::{MalachiteConfig, Multiaddr, PeerId};
use ethexe_service::config::{MalachiteCliConfig, ValidatorIdentity};
use gsigner::secp256k1::{Address, PublicKey};
use serde::Deserialize;
use std::{collections::BTreeMap, net::SocketAddr, path::PathBuf, str::FromStr};

/// Parameters for the Malachite consensus service.
///
/// All fields are `Option`-al so that a caller's CLI flags can override
/// a TOML file via [`MergeParams`]. Defaults are resolved in
/// [`MalachiteParams::into_config`].
#[derive(Clone, Debug, Default, Deserialize, Parser)]
#[serde(deny_unknown_fields)]
pub struct MalachiteParams {
    /// Listen address for the Malachite consensus libp2p swarm.
    ///
    /// This is a **separate** socket from `--network-listen-addr`
    /// (which serves the QUIC-based ethexe-network on port 20333 by
    /// default) — the Malachite swarm currently uses TCP and its own
    /// secp256k1 peer id (deterministically derived from the
    /// validator key, but distinct from the ethexe-network peer id).
    #[arg(long, aliases = &["mala-listen-addr", "malachite-listen"])]
    #[serde(rename = "listen-addr")]
    pub malachite_listen_addr: Option<SocketAddr>,

    /// Persistent peer multiaddrs the Malachite swarm should always
    /// keep connections to. Each entry must include a
    /// `/p2p/<peer_id>` suffix. Repeat the flag to add more than one
    /// peer.
    ///
    /// Example for a 3-node test on localhost:
    ///   `--malachite-persistent-peer /ip4/127.0.0.1/tcp/20335/p2p/12D3KooW...`
    ///   `--malachite-persistent-peer /ip4/127.0.0.1/tcp/20336/p2p/12D3KooW...`
    #[arg(long = "malachite-persistent-peer", aliases = &["mala-persistent-peer"])]
    #[serde(default, rename = "persistent-peers")]
    pub malachite_persistent_peers: Vec<Multiaddr>,

    /// Path to a JSON file mapping validator Ethereum addresses to
    /// their Malachite secp256k1 public keys and libp2p peer IDs.
    ///
    /// The Router contract stores the validator set as Ethereum
    /// addresses; the Malachite engine needs the matching public
    /// keys to verify votes/proposals and the peer IDs to drop
    /// proposal parts from non-validator publishers before buffering.
    /// At startup, the service loads this table and looks every
    /// on-chain validator address up in it (in router order) to
    /// build the final validator set.
    ///
    /// File format (a flat JSON object — addresses and keys are
    /// hex-encoded with `0x` prefix, peer IDs are libp2p peer-id
    /// strings):
    /// ```json
    /// {
    ///   "0xaaaa...": {
    ///     "public_key": "0x02bbbb...",
    ///     "peer_id": "16Uiu2HAm..."
    ///   }
    /// }
    /// ```
    #[arg(
        long = "validators-malachite-identities",
        aliases = &["mala-validator-identities"]
    )]
    #[serde(rename = "validator-identities")]
    pub validators_malachite_identities: Option<PathBuf>,
}

impl MalachiteParams {
    /// Converts CLI/TOML Malachite parameters into a service-ready
    /// [`MalachiteCliConfig`]. Missing fields fall back to sensible
    /// defaults from [`MalachiteConfig`].
    pub fn into_config(self) -> Result<MalachiteCliConfig> {
        let validator_identities = match self.validators_malachite_identities {
            Some(path) => load_validator_identities_table(&path)?,
            None => BTreeMap::new(),
        };
        Ok(MalachiteCliConfig {
            listen_addr: self
                .malachite_listen_addr
                .unwrap_or(MalachiteConfig::DEFAULT_LISTEN_ADDR),
            persistent_peers: self.malachite_persistent_peers,
            validator_identities,
        })
    }
}

#[derive(Deserialize)]
struct RawValidatorIdentity {
    public_key: PublicKey,
    peer_id: String,
}

/// Read a JSON file with the validator identity table. The map is
/// `{ "0x<address>": { "public_key": "0x<pubkey>", "peer_id": "<peer>" } }`.
/// Errors include the file path and offending validator for easier diagnosis.
fn load_validator_identities_table(
    path: &std::path::Path,
) -> Result<BTreeMap<Address, ValidatorIdentity>> {
    let content = std::fs::read_to_string(path).with_context(|| {
        format!(
            "failed to read malachite validator identities file at {}",
            path.display()
        )
    })?;
    let raw: BTreeMap<Address, RawValidatorIdentity> = serde_json::from_str(&content)
        .with_context(|| {
            format!(
                "failed to parse malachite validator identities file at {}",
                path.display()
            )
        })?;
    let mut identities = BTreeMap::new();
    let mut peer_ids = BTreeMap::new();
    for (addr, raw) in raw {
        let peer_id = PeerId::from_str(&raw.peer_id).with_context(|| {
            format!(
                "validator address {addr} has malformed peer_id in {}",
                path.display()
            )
        })?;
        if let Some(previous) = peer_ids.insert(peer_id, addr) {
            anyhow::bail!(
                "duplicate malachite peer_id {peer_id} in {} for validators {previous} and {addr}",
                path.display()
            );
        }
        identities.insert(
            addr,
            ValidatorIdentity {
                public_key: raw.public_key,
                peer_id,
            },
        );
    }
    Ok(identities)
}

impl MergeParams for MalachiteParams {
    fn merge(self, with: Self) -> Self {
        // Persistent peers concatenate (CLI list + file list). Empty
        // lists merge to empty, which is the same as the default.
        let mut persistent_peers = self.malachite_persistent_peers;
        persistent_peers.extend(with.malachite_persistent_peers);
        Self {
            malachite_listen_addr: self.malachite_listen_addr.or(with.malachite_listen_addr),
            malachite_persistent_peers: persistent_peers,
            validators_malachite_identities: self
                .validators_malachite_identities
                .or(with.validators_malachite_identities),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ethexe_malachite::malachite_libp2p_peer_id;
    use gsigner::schemes::secp256k1::PrivateKey;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn test_identity(seed: u8) -> (Address, PublicKey, PeerId) {
        let mut bytes = [0u8; 32];
        bytes[31] = seed;
        let private_key = PrivateKey::from_seed(bytes).expect("private key");
        (
            private_key.public_key().to_address(),
            private_key.public_key(),
            malachite_libp2p_peer_id(&private_key.to_bytes()),
        )
    }

    fn identity_json(entries: &[(Address, PublicKey, PeerId)]) -> String {
        let entries = entries
            .iter()
            .map(|(addr, public_key, peer_id)| {
                format!(r#""{addr}":{{"public_key":"{public_key}","peer_id":"{peer_id}"}}"#)
            })
            .collect::<Vec<_>>()
            .join(",");
        format!("{{{entries}}}")
    }

    fn temp_identity_file(contents: &str) -> NamedTempFile {
        let mut file = NamedTempFile::new().expect("temp identity file");
        file.write_all(contents.as_bytes())
            .expect("write identity file");
        file
    }

    #[test]
    fn validator_identities_parse_valid_input() {
        let one = test_identity(1);
        let two = test_identity(2);
        let file = temp_identity_file(&identity_json(&[one, two]));

        let identities = load_validator_identities_table(file.path()).expect("valid identities");

        assert_eq!(identities.len(), 2);
        assert_eq!(identities[&one.0].public_key, one.1);
        assert_eq!(identities[&one.0].peer_id, one.2);
        assert_eq!(identities[&two.0].public_key, two.1);
        assert_eq!(identities[&two.0].peer_id, two.2);
    }

    #[test]
    fn validator_identities_reject_malformed_peer_id() {
        let (addr, public_key, _) = test_identity(1);
        let file = temp_identity_file(&format!(
            r#"{{"{addr}":{{"public_key":"{public_key}","peer_id":"not-a-peer-id"}}}}"#
        ));

        let error = load_validator_identities_table(file.path()).unwrap_err();

        assert!(error.to_string().contains("malformed peer_id"));
        assert!(error.to_string().contains(&addr.to_string()));
    }

    #[test]
    fn validator_identities_reject_duplicate_peer_id() {
        let one = test_identity(1);
        let mut two = test_identity(2);
        two.2 = one.2;
        let file = temp_identity_file(&identity_json(&[one, two]));

        let error = load_validator_identities_table(file.path()).unwrap_err();

        assert!(error.to_string().contains("duplicate malachite peer_id"));
    }
}
