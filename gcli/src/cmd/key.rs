// This file is part of Gear.
//
// Copyright (C) 2021-2022 Gear Technologies Inc.
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

//! command `key`
use crate::{keystore::key::Key as KeyT, result::Result};
use clap::Parser;
use gsdk::ext::{
    sp_core::{ecdsa, ed25519, sr25519, Pair},
    sp_runtime::traits::IdentifyAccount,
};
use std::{fmt::Display, result::Result as StdResult, str::FromStr};

/// Cryptography scheme
#[derive(Debug, Clone)]
pub enum Scheme {
    Ecdsa,
    Ed25519,
    Sr25519,
}

impl FromStr for Scheme {
    type Err = &'static str;

    fn from_str(s: &str) -> StdResult<Self, Self::Err> {
        Ok(match s.to_ascii_lowercase().as_str() {
            "ecdsa" => Scheme::Ecdsa,
            "ed25519" => Scheme::Ed25519,
            _ => Scheme::Sr25519,
        })
    }
}

#[derive(Debug, Parser)]
pub enum Action {
    /// Generate a random account
    Generate,

    /// Generate a random node libp2p key
    #[cfg(feature = "node-key")]
    #[clap(name = "generate-node-key")]
    GenerateNodeKey,

    /// Gets a public key and a SS58 address from the provided Secret URI
    Inspect {
        /// Secret URI of the key
        suri: String,
    },

    /// Print the peer ID corresponding to the node key in the given file
    #[cfg(feature = "node-key")]
    InspectNodeKey {
        /// Hex encoding of the secret key
        secret: String,
    },

    /// Sign a message, with a given (secret) key
    Sign {
        /// Secret URI of the key
        suri: String,
        /// Message to sign
        message: String,
    },

    /// Verify a signature for a message
    Verify {
        /// Signature to verify
        signature: String,
        /// Raw message
        message: String,
        /// Public key of the signer of this signature
        pubkey: String,
    },
}

/// Keypair utils
#[derive(Debug, Parser)]
pub struct Key {
    /// Cryptography scheme
    #[arg(short, long, default_value = "sr25519")]
    scheme: Scheme,
    /// Key actions
    #[command(subcommand)]
    action: Action,
}

macro_rules! match_scheme {
    ($scheme:expr, $op:tt ($($arg:ident),*), $res:ident, $repeat:expr) => {
        match $scheme {
            Scheme::Ecdsa => {
                let $res = KeyT::$op::<ecdsa::Pair>($($arg),*)?;
                $repeat
            }
            Scheme::Ed25519 => {
                let $res = KeyT::$op::<ed25519::Pair>($($arg),*)?;
                $repeat
            }
            Scheme::Sr25519 => {
                let $res = KeyT::$op::<sr25519::Pair>($($arg),*)?;
                $repeat
            }
        }
    };
}

impl Key {
    /// # NOTE
    ///
    /// Reserved the `passwd` for getting suri from cache.
    pub fn exec(&self, passwd: Option<&str>) -> Result<()> {
        match &self.action {
            Action::Generate => self.generate(passwd)?,
            #[cfg(feature = "node-key")]
            Action::GenerateNodeKey => Self::generate_node_key(),
            Action::Inspect { suri } => self.inspect(suri, passwd)?,
            #[cfg(feature = "node-key")]
            Action::InspectNodeKey { secret } => Self::inspect_node_key(secret)?,
            Action::Sign { suri, message } => self.sign(suri, message, passwd)?,
            Action::Verify {
                signature,
                message,
                pubkey,
            } => self.verify(signature, message, pubkey)?,
        }

        Ok(())
    }

    fn generate(&self, passwd: Option<&str>) -> Result<()> {
        match_scheme!(self.scheme, generate_with_phrase(passwd), res, {
            let (pair, phrase, seed) = res;
            let signer = pair.signer();

            Self::info(&format!("Secret Phrase `{phrase}`"), signer, Some(seed));
        });

        Ok(())
    }

    #[cfg(feature = "node-key")]
    fn generate_node_key() {
        use libp2p::identity::{ed25519::Keypair, PublicKey};
        let pair = Keypair::generate();

        println!("Secret:  0x{}", hex::encode(pair.secret().as_ref()));
        println!(
            "Peer ID: {}",
            PublicKey::Ed25519(pair.public()).to_peer_id()
        );
    }

    fn info<P>(title: &str, signer: &P, seed: Option<Vec<u8>>)
    where
        P: Pair,
        <P as Pair>::Public: IdentifyAccount,
        <<P as Pair>::Public as IdentifyAccount>::AccountId: Display,
    {
        let ss = if let Some(seed) = seed {
            seed
        } else {
            signer.to_raw_vec()
        };

        println!("{title} is account:");
        println!("	Secret Seed:  0x{}", hex::encode(&ss[..32]));
        println!("	Public key:   0x{}", hex::encode(signer.public()));
        println!("	SS58 Address: {}", signer.public().into_account());
    }

    fn inspect(&self, suri: &str, passwd: Option<&str>) -> Result<()> {
        let key = KeyT::from_string(suri);
        let key_ref = &key;
        match_scheme!(self.scheme, pair(key_ref, passwd), pair, {
            Self::info(&format!("Secret Key URI `{suri}`"), pair.0.signer(), pair.1)
        });

        Ok(())
    }

    #[cfg(feature = "node-key")]
    fn inspect_node_key(secret: &str) -> Result<()> {
        use libp2p::identity::{
            ed25519::{Keypair, SecretKey},
            PublicKey,
        };
        let pair = Keypair::from(
            SecretKey::from_bytes(&mut hex::decode(secret)?)
                .map_err(|_| crate::result::Error::BadNodeKey)?,
        );

        println!(
            "Peer ID: {}",
            PublicKey::Ed25519(pair.public()).to_peer_id()
        );
        Ok(())
    }

    fn sign(&self, suri: &str, message: &str, passwd: Option<&str>) -> Result<()> {
        let key = KeyT::from_string(suri);
        let key_ref = &key;

        match_scheme!(self.scheme, pair(key_ref, passwd), pair, {
            let signer = pair.0.signer();
            let sig = signer.sign(message.as_bytes());

            println!("Message: {message}");
            println!("Signature: {}", hex::encode::<&[u8]>(sig.as_ref()));
            Self::info("The signer of this signature", signer, pair.1)
        });

        Ok(())
    }

    fn verify(&self, sig: &str, message: &str, pubkey: &str) -> Result<()> {
        let arr = [sig, pubkey]
            .iter()
            .map(|i| hex::decode(i.trim_start_matches("0x")))
            .collect::<StdResult<Vec<Vec<u8>>, hex::FromHexError>>()?;
        let [sig, msg, pubkey] = [&arr[0], message.as_bytes(), &arr[1]];

        match_scheme!(self.scheme, verify(sig, msg, pubkey), res, {
            println!("Result: {res}");
        });

        Ok(())
    }
}
