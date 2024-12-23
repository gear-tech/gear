// This file is part of Gear.
//
// Copyright (C) 2024 Gear Technologies Inc.
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

use super::utils;
use crate::params::Params;
use anyhow::{anyhow, bail, Context, Result};
use clap::{Parser, Subcommand};
use ethexe_signer::{Signature, Signer, ToDigest as _};
use std::path::PathBuf;

#[derive(Debug, Parser)]
pub struct KeyCommand {
    #[arg(short, long)]
    pub key_store: Option<PathBuf>,

    #[command(subcommand)]
    pub command: KeySubcommand,
}

impl KeyCommand {
    pub fn with_params(mut self, params: Params) -> Self {
        self.key_store = self
            .key_store
            .take()
            .or_else(|| Some(params.node.unwrap_or_default().keys_dir()));

        self
    }

    pub fn exec(self) -> Result<()> {
        let key_store = self.key_store.expect("must never be empty after merging");

        let signer = Signer::new(key_store).with_context(|| "failed to create signer")?;

        match self.command {
            KeySubcommand::Clear => {
                let len = signer.list_keys()?.len();

                signer
                    .clear_keys()
                    .with_context(|| "failed to clear keys")?;

                println!("Removed {len} keys");
            }
            KeySubcommand::Generate => {
                // TODO: remove println from there.
                let public = signer
                    .generate_key()
                    .with_context(|| "failed to generate new keypair")?;

                println!("Public key: {public}");
                println!("Ethereum address: {}", public.to_address());
            }
            KeySubcommand::Insert { private_key } => {
                let private = private_key
                    .parse()
                    .with_context(|| "invalid `private-key`")?;

                let public = signer
                    .add_key(private)
                    .with_context(|| "failed to add key")?;

                println!("Public key: {public}");
                println!("Ethereum address: {}", public.to_address());
            }
            KeySubcommand::List => {
                let publics = signer.list_keys().with_context(|| "failed to list keys")?;

                println!("[ No | {:^66} | {:^42} ]", "Public key", "Ethereum address");

                for (i, public) in publics.into_iter().enumerate() {
                    println!("[ {:<2} | {public} | {} ]", i + 1, public.to_address());
                }
            }
            KeySubcommand::Recover { message, signature } => {
                let message =
                    utils::hex_str_to_vec(message).with_context(|| "invalid `message`")?;
                let signature =
                    utils::hex_str_to_vec(signature).with_context(|| "invalid `signature`")?;

                let signature_bytes: [u8; 65] = signature
                    .try_into()
                    .map_err(|_| anyhow!("signature isn't 65 bytes len"))
                    .with_context(|| "invalid `signature`")?;

                let signature = unsafe { Signature::from_bytes(signature_bytes) };

                let public = signature
                    .recover_from_digest(message.to_digest())
                    .with_context(|| "failed to recover signature from digest")?;

                println!("Signed by: {public}");
                println!("Ethereum address: {}", public.to_address());
            }
            KeySubcommand::Show { key } => {
                let key = key.strip_prefix("0x").unwrap_or(&key);

                let public = if key.len() == 66 {
                    key.parse().with_context(|| "invalid `key`")?
                } else if key.len() == 40 {
                    let mut address_bytes = [0u8; 20];
                    hex::decode_to_slice(key, &mut address_bytes)
                        .map_err(|e| anyhow!("Failed to parse eth address hex: {e}"))
                        .with_context(|| "invalid `key`")?;

                    signer
                        .get_key_by_addr(address_bytes.into())?
                        .ok_or_else(|| anyhow!("Unrecognized eth address"))
                        .with_context(|| "invalid `key`")?
                } else {
                    bail!("Invalid key length: should be 33 bytes public key or 20 bytes eth address ");
                };

                let private = signer
                    .get_private_key(public)
                    .with_context(|| "failed to get private key")?;

                println!("Secret key: {}", hex::encode(private.0));
                println!("Public key: {public}");
                println!("Ethereum address: {}", public.to_address());
            }
            KeySubcommand::Sign { key, message } => {
                let public = key.parse().with_context(|| "invalid `key`")?;

                let message =
                    utils::hex_str_to_vec(message).with_context(|| "invalid `message`")?;

                let signature = signer
                    .sign(public, &message)
                    .with_context(|| "failed to sign message")?;

                println!("Signature: {signature}");
            }
        }

        Ok(())
    }
}

#[derive(Debug, Subcommand)]
pub enum KeySubcommand {
    Clear,
    Generate,
    Insert {
        #[arg()]
        private_key: String,
    },
    List,
    Recover {
        #[arg(short, long)]
        message: String,
        #[arg(short, long)]
        signature: String,
    },
    Show {
        #[arg()]
        key: String,
    },
    Sign {
        #[arg(short, long)]
        key: String,
        #[arg(short, long)]
        message: String,
    },
}
