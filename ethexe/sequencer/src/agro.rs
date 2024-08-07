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
use ethexe_signer::{Address, AsDigest, Digest, PrivateKey, PublicKey, Signature, Signer};
use parity_scale_codec::{Decode, Encode};

pub trait CommitmentsDigestSigner {
    fn sign_commitments_digest(
        &self,
        commitments_digest: Digest,
        pub_key: PublicKey,
        router_address: Address,
    ) -> Result<Signature>;

    fn recover_from_commitments_digest(
        commitments_digest: Digest,
        signature: &Signature,
        router_address: Address,
    ) -> Result<Address> {
        recover_from_commitments_digest(commitments_digest, signature, router_address)
    }
}

impl CommitmentsDigestSigner for Signer {
    fn sign_commitments_digest(
        &self,
        commitments_digest: Digest,
        pub_key: PublicKey,
        router_address: Address,
    ) -> Result<Signature> {
        let digest = to_router_digest(commitments_digest, router_address);
        self.sign_digest(pub_key, digest)
    }
}

impl CommitmentsDigestSigner for PrivateKey {
    fn sign_commitments_digest(
        &self,
        commitments_digest: Digest,
        _pub_key: PublicKey,
        router_address: Address,
    ) -> Result<Signature> {
        let digest = to_router_digest(commitments_digest, router_address);
        Signature::create_for_digest(*self, digest)
    }
}

#[derive(Clone, Debug, Encode, Decode, PartialEq, Eq)]
pub struct AggregatedCommitments<D: AsDigest> {
    pub commitments: Vec<D>,
    pub signature: Signature,
}

impl<T: AsDigest> AggregatedCommitments<T> {
    pub fn aggregate_commitments(
        commitments: Vec<T>,
        signer: &impl CommitmentsDigestSigner,
        pub_key: PublicKey,
        router_address: Address,
    ) -> Result<AggregatedCommitments<T>> {
        let signature =
            signer.sign_commitments_digest(commitments.as_digest(), pub_key, router_address)?;

        Ok(AggregatedCommitments {
            commitments,
            signature,
        })
    }

    pub fn recover(&self, router_address: Address) -> Result<Address> {
        recover_from_commitments_digest(
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

fn recover_from_commitments_digest(
    commitments_digest: Digest,
    signature: &Signature,
    router_address: Address,
) -> Result<Address> {
    signature
        .recover_from_digest(to_router_digest(commitments_digest, router_address))
        .map(|k| k.to_address())
}

fn to_router_digest(commitments_digest: Digest, router_address: Address) -> Digest {
    [
        [0x19, 0x00].as_ref(),
        router_address.0.as_ref(),
        commitments_digest.as_ref(),
    ]
    .concat()
    .as_digest()
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
        let signature = signer
            .sign_commitments_digest(digest, pub_key, router_address)
            .unwrap();
        let recovered =
            recover_from_commitments_digest(digest, &signature, router_address).unwrap();

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
