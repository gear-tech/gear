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

//! command `login`
use crate::{keystore, result::Result};
use clap::Parser;
use std::path::PathBuf;

/// Log in to account
#[derive(Debug, Parser)]
pub struct Login {
    /// The default keystore path is ~/.gear/keystore and ~/.gear/keystore.json
    ///
    /// @WARNING: THIS WILL ONLY BE SECURE IF THE keystore IS SECURE.
    /// when you have NO PASSWORD, If it can be got by an attacker then
    /// they can also get your key.
    ///
    /// - If `s` is a possibly `0x` prefixed 64-digit hex string, then it will be interpreted
    /// directly as a `MiniSecretKey` (aka "seed" in `subkey`).
    ///
    /// - If `s` is a valid BIP-39 key phrase of 12, 15, 18, 21 or 24 words, then the key will
    /// be derived from it. In this case:
    ///   - the phrase may be followed by one or more items delimited by `/` characters.
    ///   - the path may be followed by `///`, in which case everything after the `///` is treated
    /// as a password.
    ///
    /// - If `s` begins with a `/` character it is prefixed with the Substrate public `DEV_PHRASE`
    ///   and
    /// interpreted as above.
    pub suri: Option<String>,

    /// The path of the json file generated from the polkadotjs wallet.
    pub json: Option<PathBuf>,

    /// password of the signer account
    #[arg(short, long)]
    pub passwd: Option<String>,
}

impl Login {
    /// exec command login
    pub fn exec(&self) -> Result<()> {
        let signer = match (&self.suri, &self.json) {
            (Some(suri), _) => keystore::login(suri, self.passwd.as_ref().map(|p| p.as_ref()))?,
            (None, Some(json)) => {
                keystore::decode_json_file(json, self.passwd.as_ref().map(|p| p.as_ref()))?
            }
            _ => {
                if let Ok(pair) = keystore::cache(self.passwd.as_deref()) {
                    pair
                } else {
                    keystore::generate(self.passwd.as_ref().map(|p| p.as_ref()))?
                }
            }
        };

        println!("Successfully logged in as {}!", signer.account_id());
        Ok(())
    }
}
