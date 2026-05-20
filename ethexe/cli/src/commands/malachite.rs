// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Implementation of the `ethexe malachite` command family.
//!
//! Currently only exposes [`MalachiteSubcommand::PeerId`], which lets
//! operators derive the libp2p peer_id of the Malachite swarm
//! offline (without booting the node) for a given validator key.
//! That value is what fills the `/p2p/<peer_id>` suffix of a
//! `--malachite-persistent-peer` multiaddr.

use crate::params::Params;
use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use ethexe_malachite::malachite_libp2p_peer_id;
use gsigner::secp256k1::{PublicKey, Signer};
use std::path::PathBuf;

/// Malachite-specific helper commands.
#[derive(Debug, Parser)]
pub struct MalachiteCommand {
    /// Validator keystore directory (defaults to the node's standard
    /// keys directory derived from `--base-path`).
    #[arg(short, long)]
    pub key_store: Option<PathBuf>,

    /// Subcommand to run.
    #[command(subcommand)]
    pub command: MalachiteSubcommand,
}

#[derive(Debug, Subcommand)]
pub enum MalachiteSubcommand {
    /// Print the libp2p peer_id this validator key uses on the
    /// Malachite swarm. The value is derived deterministically from
    /// the validator secret and is independent of the on-chain
    /// validator address.
    PeerId {
        /// Validator public key whose Malachite peer_id you want to
        /// derive (must be present in the keystore).
        validator: PublicKey,
    },
}

impl MalachiteCommand {
    /// Merge the command with the provided params (fill in the
    /// keystore path from the node base path if the user didn't pass
    /// `--key-store` explicitly).
    pub fn with_params(mut self, params: Params) -> Self {
        let node = params.node.unwrap_or_default();
        self.key_store = self.key_store.take().or_else(|| Some(node.keys_dir()));
        self
    }

    pub fn exec(self) -> Result<()> {
        let key_store = self.key_store.expect("must never be empty after merging");

        match self.command {
            MalachiteSubcommand::PeerId { validator } => {
                let signer = Signer::fs(key_store).context("opening validator keystore")?;
                let secret = signer
                    .private_key(validator)
                    .context("validator key not found in keystore")?
                    .to_bytes();

                let peer_id = malachite_libp2p_peer_id(&secret);

                println!("{peer_id}");
                println!();
                println!(
                    "Example persistent-peer multiaddr (replace IP/port for each peer):\n  \
                     /ip4/127.0.0.1/tcp/20334/p2p/{peer_id}"
                );
            }
        }

        Ok(())
    }
}
