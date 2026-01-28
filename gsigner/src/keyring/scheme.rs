// This file is part of Gear.
//
// Copyright (C) 2021-2026 Gear Technologies Inc.
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

//! Adapter trait allowing schemes to integrate with the generic [`Keyring`].

use super::KeystoreEntry;
use crate::{error::Result, traits::SignatureScheme};
use serde::{Serialize, de::DeserializeOwned};

/// Signature schemes that can be stored in the JSON keyring.
pub trait KeyringScheme: SignatureScheme {
    /// Concrete keystore representation for this scheme.
    type Keystore: KeystoreEntry + Clone + Serialize + DeserializeOwned + Send + Sync + 'static;

    /// Directory namespace used to segregate scheme keyrings on disk.
    fn namespace() -> &'static str;

    /// Build a keystore representation from a private key.
    fn keystore_from_private(
        name: &str,
        private_key: &Self::PrivateKey,
        password: Option<&str>,
    ) -> Result<Self::Keystore>;

    /// Recover the private key from a keystore.
    fn keystore_private(
        keystore: &Self::Keystore,
        password: Option<&str>,
    ) -> Result<Self::PrivateKey>;

    /// Recover the public key from a keystore.
    fn keystore_public(keystore: &Self::Keystore) -> Result<Self::PublicKey>;

    /// Recover the address from a keystore.
    fn keystore_address(keystore: &Self::Keystore) -> Result<Self::Address>;
}
