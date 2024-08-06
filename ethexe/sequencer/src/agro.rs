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

//! Abstract commitment aggregator.

use anyhow::Result;
use ethexe_signer::{Address, AsDigest, Digest, PublicKey, Signature, Signer};
use parity_scale_codec::{Decode, Encode};
use std::fmt;

#[derive(Clone, Debug, Encode, Decode, PartialEq, Eq)]
pub struct AggregatedCommitments<D: AsDigest> {
    pub commitments: Vec<D>,
    pub signature: Signature,
}

impl<T: AsDigest> AggregatedCommitments<T> {
    pub fn aggregate_commitments(
        commitments: Vec<T>,
        signer: &Signer,
        pub_key: PublicKey,
        router_address: Address,
    ) -> Result<AggregatedCommitments<T>> {
        let signature = sign_digest(commitments.as_digest(), signer, pub_key, router_address)?;

        Ok(AggregatedCommitments {
            commitments,
            signature,
        })
    }

    pub fn recover(&self, router_address: Address) -> Result<Address> {
        recover_from_digest(
            self.commitments.as_digest(),
            &self.signature,
            router_address,
        )
    }

    pub fn len(&self) -> usize {
        self.commitments.len()
    }

    pub fn is_empty(&self) -> bool {
        self.commitments.is_empty()
    }
}

pub fn sign_digest(
    commitments_digest: Digest,
    signer: &Signer,
    pub_key: PublicKey,
    router_address: Address,
) -> Result<Signature> {
    signer.sign_digest(pub_key, digest(commitments_digest, router_address))
}

pub fn recover_from_digest(
    commitments_digest: Digest,
    signature: &Signature,
    router_address: Address,
) -> Result<Address> {
    signature
        .recover_from_digest(digest(commitments_digest, router_address))
        .map(|k| k.to_address())
}

fn digest(commitments_digest: Digest, router_address: Address) -> Digest {
    [
        [0x19, 0x00].as_ref(),
        router_address.0.as_ref(),
        commitments_digest.as_ref(),
    ]
    .concat()
    .as_digest()
}

#[derive(Clone)]
pub struct MultisignedCommitments<D> {
    pub commitments: Vec<D>,
    pub sources: Vec<Address>,
    pub signatures: Vec<Signature>,
}

impl<D: fmt::Debug> fmt::Debug for MultisignedCommitments<D> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "MultisignedCommitments {{ commitments: {:?}, sources: {:?}, signatures: {:?} }}",
            self.commitments, self.sources, self.signatures
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ethexe_signer::PrivateKey;
    use std::str::FromStr;

    #[derive(Clone, Debug)]
    pub struct MyComm([u8; 2]);

    impl AsDigest for MyComm {
        fn as_digest(&self) -> Digest {
            self.0.as_digest()
        }
    }

    #[test]
    fn test_sign_digest() {
        let key_store = tempfile::tempdir().unwrap();
        let signer = Signer::new(key_store.path().to_path_buf()).unwrap();

        let private_key = PrivateKey::from_str(
            "4c0883a69102937d6231471b5dbb6204fe51296170827936ea5cce4b76994b0f",
        )
        .unwrap();
        let pub_key = signer.add_key(private_key).unwrap();

        let router_address = Address([0x01; 20]);
        let commitments = vec![MyComm([1, 2]), MyComm([3, 4])];

        let digest = commitments.as_digest();
        let signature = sign_digest(digest, &signer, pub_key, router_address).unwrap();
        let recovered = recover_from_digest(digest, &signature, router_address).unwrap();

        assert_eq!(recovered, pub_key.to_address());
    }

    #[test]
    fn test_aggregated_commitments() {
        let key_store = tempfile::tempdir().unwrap();
        let signer = Signer::new(key_store.path().to_path_buf()).unwrap();

        let private_key = PrivateKey::from_str(
            "4c0883a69102937d6231471b5dbb6204fe51296170827936ea5cce4b76994b0f",
        )
        .unwrap();
        let pub_key = signer.add_key(private_key).unwrap();

        let router_address = Address([0x01; 20]);
        let commitments = vec![MyComm([1, 2]), MyComm([3, 4])];

        let agg = AggregatedCommitments::aggregate_commitments(
            commitments,
            &signer,
            pub_key,
            router_address,
        )
        .unwrap();
        let recovered = agg.recover(router_address).unwrap();

        assert_eq!(recovered, pub_key.to_address());
    }
}
