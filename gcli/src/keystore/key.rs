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

//! SURI manager
use crate::result::{Error, Result};
use gsdk::{
    config::GearConfig,
    ext::{
        sp_core::Pair,
        sp_runtime::{MultiSignature, MultiSigner},
    },
    PairSigner,
};
use keyring::Entry;
use std::env;

type SignerAndSeed<P> = (PairSigner<GearConfig, P>, Option<Vec<u8>>);

/// @WARNING: THIS WILL ONLY BE SECURE IF THE keystore IS SECURE.
/// when you have NO PASSWORD, If it can be got by an attacker then
/// they can also get your key.
pub struct Key(String);

impl Key {
    /// Get keyring entry
    pub fn keyring() -> Result<Entry> {
        let username = whoami::username();
        let application = env::current_exe()?
            .file_name()
            .ok_or(Error::InvalidExecutable)?
            .to_string_lossy()
            .to_string();

        Ok(keyring::Entry::new(&application, &username))
    }

    /// New key from string
    pub fn from_string(suri: &str) -> Self {
        Self(suri.into())
    }

    /// New key from keyring
    ///
    /// # TODO
    ///
    /// prepare for #20
    #[allow(unused)]
    pub fn from_keyring() -> Result<Self> {
        let entry = Self::keyring()?;

        Ok(Self(entry.get_password()?))
    }

    /// Generate pair with phrase
    pub fn generate_with_phrase<P>(
        passwd: Option<&str>,
    ) -> Result<(PairSigner<GearConfig, P>, String, Vec<u8>)>
    where
        P: Pair,
        MultiSignature: From<<P as Pair>::Signature>,
        MultiSigner: From<<P as Pair>::Public>,
    {
        let pair = P::generate_with_phrase(passwd);
        Ok((PairSigner::new(pair.0), pair.1, pair.2.as_ref().to_vec()))
    }

    /// Get keypair from key
    pub fn pair<P>(&self, passwd: Option<&str>) -> Result<SignerAndSeed<P>>
    where
        P: Pair,
        MultiSignature: From<<P as Pair>::Signature>,
        MultiSigner: From<<P as Pair>::Public>,
    {
        let (pair, seed) =
            P::from_string_with_seed(&self.0, passwd).map_err(|_| Error::InvalidSecret)?;
        Ok((PairSigner::new(pair), seed.map(|s| s.as_ref().to_vec())))
    }

    /// Sign messages
    pub fn sign<P>(&self, msg: &str, passwd: Option<&str>) -> Result<Vec<u8>>
    where
        P: Pair,
    {
        let pair = P::from_string(&self.0, passwd).map_err(|_| Error::InvalidSecret)?;
        // # Note
        //
        // using `msg.as_bytes()` here, will not decode the hex encoding
        // messages here.
        Ok(pair.sign(msg.as_bytes()).as_ref().to_vec())
    }

    /// Verify messages
    pub fn verify<P>(sig: &[u8], message: &[u8], pubkey: &[u8]) -> Result<bool>
    where
        P: Pair,
    {
        Ok(P::verify_weak(sig, message, pubkey))
    }
}
