// This file is part of Gear.
//
// Copyright (C) 2025 Gear Technologies Inc.
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

//! Distributed Key Generation (DKG) types backed by the ROAST/FROST library.

use crate::{Address, ToDigest};
use alloc::{
    collections::{BTreeMap, BTreeSet},
    vec::Vec,
};
use core::hash::{Hash, Hasher};
use parity_scale_codec::{Decode, Encode};
use roast_secp256k1_evm::frost::{
    Identifier, VerifyingKey,
    keys::{
        KeyPackage, PublicKeyPackage, VerifiableSecretSharingCommitment,
        dkg::{round1, round2},
    },
};
use sha3::{Digest, Keccak256};

pub type DkgIdentifier = Identifier;
pub type DkgKeyPackage = KeyPackage;
pub type DkgPublicKeyPackage = PublicKeyPackage;
pub type DkgRound1Package = round1::Package;
pub type DkgRound2Package = round2::Package;
pub type DkgVerifyingKey = VerifyingKey;
pub type DkgVssCommitment = VerifiableSecretSharingCommitment;

#[derive(Clone, Debug, Encode, Decode, PartialEq, Eq)]
pub struct DkgShare {
    pub era: u64,
    pub identifier: DkgIdentifier,
    pub index: u16,
    pub signing_share: Vec<u8>,
    pub verifying_share: Vec<u8>,
    pub threshold: u16,
}

#[derive(Clone, Copy, Debug, Encode, Decode, PartialEq, Eq, Hash)]
pub struct DkgSessionId {
    pub era: u64,
}

impl ToDigest for DkgSessionId {
    fn update_hasher(&self, hasher: &mut Keccak256) {
        hasher.update(self.era.to_be_bytes());
    }
}

/// Round 1 broadcast message.
#[derive(Clone, Debug, Encode, Decode, PartialEq, Eq)]
pub struct DkgRound1 {
    pub session: DkgSessionId,
    pub package: DkgRound1Package,
    pub temp_public_key: DkgVerifyingKey,
}

impl Hash for DkgRound1 {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write(&self.encode());
    }
}

impl ToDigest for DkgRound1 {
    fn update_hasher(&self, hasher: &mut Keccak256) {
        hasher.update(self.encode());
    }
}

/// Round 2 packages from a single participant (encrypted per receiver).
#[derive(Clone, Debug, Encode, Decode, PartialEq, Eq)]
pub struct DkgRound2 {
    pub session: DkgSessionId,
    pub packages: BTreeMap<DkgIdentifier, [u8; 32]>,
}

impl Hash for DkgRound2 {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write(&self.encode());
    }
}

impl ToDigest for DkgRound2 {
    fn update_hasher(&self, hasher: &mut Keccak256) {
        hasher.update(self.encode());
    }
}

/// Round 2 culprits report (cheater detection).
#[derive(Clone, Debug, Encode, Decode, PartialEq, Eq)]
pub struct DkgRound2Culprits {
    pub session: DkgSessionId,
    pub culprits: BTreeSet<DkgIdentifier>,
    pub temp_secret_key: Vec<u8>,
}

impl Hash for DkgRound2Culprits {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write(&self.encode());
    }
}

impl ToDigest for DkgRound2Culprits {
    fn update_hasher(&self, hasher: &mut Keccak256) {
        hasher.update(self.encode());
    }
}

/// Complaint about an invalid share in Round 2.
#[derive(Clone, Debug, Encode, Decode, PartialEq, Eq)]
pub struct DkgComplaint {
    pub session: DkgSessionId,
    pub complainer: Address,
    pub offender: Address,
    pub reason: Vec<u8>,
}

impl Hash for DkgComplaint {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write(&self.encode());
    }
}

impl ToDigest for DkgComplaint {
    fn update_hasher(&self, hasher: &mut Keccak256) {
        hasher.update(self.encode());
    }
}

/// Justification for a complaint (reveals share/proof).
#[derive(Clone, Debug, Encode, Decode, PartialEq, Eq)]
pub struct DkgJustification {
    pub session: DkgSessionId,
    pub complainer: Address,
    pub offender: Address,
    pub share: [u8; 32],
    pub proof: Vec<u8>,
}

impl Hash for DkgJustification {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write(&self.encode());
    }
}

impl ToDigest for DkgJustification {
    fn update_hasher(&self, hasher: &mut Keccak256) {
        hasher.update(self.encode());
    }
}
