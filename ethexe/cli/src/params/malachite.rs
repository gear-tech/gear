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
use ethexe_malachite::{MalachiteConfig, Multiaddr};
use ethexe_service::config::MalachiteCliConfig;
use gsigner::secp256k1::{Address, PublicKey};
use serde::Deserialize;
use std::{collections::BTreeMap, net::SocketAddr, path::PathBuf};

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
    /// their Malachite secp256k1 public keys.
    ///
    /// The Router contract stores the validator set as Ethereum
    /// addresses; the Malachite engine needs the matching public
    /// keys to verify votes and proposals. At startup, the service
    /// loads this table and looks every on-chain validator address
    /// up in it (in router order) to build the final validator set.
    ///
    /// File format (a flat JSON object — both address and key are
    /// hex-encoded with `0x` prefix):
    /// ```json
    /// {
    ///   "0xaaaa...": "0x02bbbb...",
    ///   "0xcccc...": "0x03dddd..."
    /// }
    /// ```
    #[arg(long = "validators-malachite-pub-keys", aliases = &["mala-validator-keys"])]
    #[serde(rename = "validator-pub-keys")]
    pub validators_malachite_pub_keys: Option<PathBuf>,
}

impl MalachiteParams {
    /// Converts CLI/TOML Malachite parameters into a service-ready
    /// [`MalachiteCliConfig`]. Missing fields fall back to sensible
    /// defaults from [`MalachiteConfig`].
    pub fn into_config(self) -> Result<MalachiteCliConfig> {
        let validator_pub_keys = match self.validators_malachite_pub_keys {
            Some(path) => load_validator_pub_keys_table(&path)?,
            None => BTreeMap::new(),
        };
        Ok(MalachiteCliConfig {
            listen_addr: self
                .malachite_listen_addr
                .unwrap_or(MalachiteConfig::DEFAULT_LISTEN_ADDR),
            persistent_peers: self.malachite_persistent_peers,
            validator_pub_keys,
        })
    }
}

/// Read a JSON file with the validator-pubkey table. The map is
/// `{ "0x<address>": "0x<pubkey>" }`. Errors include the file path
/// for easier diagnosis.
fn load_validator_pub_keys_table(path: &std::path::Path) -> Result<BTreeMap<Address, PublicKey>> {
    let content = std::fs::read_to_string(path).with_context(|| {
        format!(
            "failed to read malachite validator pub keys file at {}",
            path.display()
        )
    })?;
    serde_json::from_str(&content).with_context(|| {
        format!(
            "failed to parse malachite validator pub keys file at {}",
            path.display()
        )
    })
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
            validators_malachite_pub_keys: self
                .validators_malachite_pub_keys
                .or(with.validators_malachite_pub_keys),
        }
    }
}
