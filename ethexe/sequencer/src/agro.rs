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

use anyhow::{anyhow, Context, Result};
use ethexe_signer::{Address, Digest, PublicKey, Signature, Signer, ToDigest};
use indexmap::IndexSet;
use parity_scale_codec::{Decode, Encode};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Encode, Decode, PartialEq, Eq)]
pub struct AggregatedCommitments<D: ToDigest> {
    pub commitments: Vec<D>,
    pub signature: Signature,
}

impl<T: ToDigest> AggregatedCommitments<T> {
    pub fn aggregate_commitments(
        commitments: Vec<T>,
        signer: &Signer,
        pub_key: PublicKey,
        router_address: Address,
    ) -> Result<AggregatedCommitments<T>> {
        let signature =
            sign_commitments_digest(commitments.to_digest(), signer, pub_key, router_address)?;

        Ok(AggregatedCommitments {
            commitments,
            signature,
        })
    }

    pub fn recover(&self, router_address: Address) -> Result<Address> {
        recover_from_commitments_digest(
            self.commitments.to_digest(),
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

pub(crate) struct MultisignedCommitmentDigests {
    digest: Digest,
    digests: IndexSet<Digest>,
    signatures: BTreeMap<Address, Signature>,
}

impl MultisignedCommitmentDigests {
    pub fn new(digests: IndexSet<Digest>) -> Result<Self> {
        if digests.is_empty() {
            return Err(anyhow!("Empty commitments digests"));
        }

        Ok(Self {
            digest: digests.iter().collect(),
            digests,
            signatures: BTreeMap::new(),
        })
    }

    pub fn append_signature_with_check(
        &mut self,
        digest: Digest,
        signature: Signature,
        router_address: Address,
        check_origin: impl FnOnce(Address) -> Result<()>,
    ) -> Result<()> {
        if self.digest != digest {
            return Err(anyhow!("Aggregated commitments digest mismatch"));
        }

        let origin = recover_from_commitments_digest(digest, &signature, router_address)
            .context("failed to recover signature origin")?;
        check_origin(origin).context("failed commitment digests signature origin check")?;

        self.signatures.insert(origin, signature);

        Ok(())
    }

    pub fn digests(&self) -> &IndexSet<Digest> {
        &self.digests
    }

    pub fn signatures(&self) -> &BTreeMap<Address, Signature> {
        &self.signatures
    }
}

pub(crate) struct MultisignedCommitments<C: ToDigest> {
    commitments: Vec<C>,
    signatures: BTreeMap<Address, Signature>,
}

impl<C: ToDigest> MultisignedCommitments<C> {
    pub fn from_multisigned_digests(
        multisigned: MultisignedCommitmentDigests,
        get_commitment: impl FnMut(Digest) -> C,
    ) -> Self {
        let MultisignedCommitmentDigests {
            digests,
            signatures,
            ..
        } = multisigned;

        Self {
            commitments: digests.into_iter().map(get_commitment).collect(),
            signatures,
        }
    }

    pub fn commitments(&self) -> &[C] {
        &self.commitments
    }

    pub fn into_parts(self) -> (Vec<C>, BTreeMap<Address, Signature>) {
        (self.commitments, self.signatures)
    }
}

pub fn sign_commitments_digest(
    commitments_digest: Digest,
    signer: &Signer,
    pub_key: PublicKey,
    router_address: Address,
) -> Result<Signature> {
    let digest = to_router_digest(commitments_digest, router_address);
    signer
        .sign_digest(pub_key, digest)
        .context("failed to sign commitments digest")
}

fn recover_from_commitments_digest(
    commitments_digest: Digest,
    signature: &Signature,
    router_address: Address,
) -> Result<Address> {
    signature
        .recover_from_digest(to_router_digest(commitments_digest, router_address))
        .map(|k| k.to_address())
        .context("failed to recover address from commitments digest")
}

fn to_router_digest(commitments_digest: Digest, router_address: Address) -> Digest {
    // See explanation: https://eips.ethereum.org/EIPS/eip-191
    [
        [0x19, 0x00].as_ref(),
        router_address.0.as_ref(),
        commitments_digest.as_ref(),
    ]
    .concat()
    .to_digest()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ethexe_signer::{
        sha3::{Digest as _, Keccak256},
        PrivateKey,
    };
    use std::str::FromStr;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct MyComm([u8; 2]);

    impl ToDigest for MyComm {
        fn update_hasher(&self, hasher: &mut Keccak256) {
            hasher.update(self.0);
        }
    }

    #[test]
    fn test_sign_digest() {
        let signer = Signer::tmp();

        let private_key = PrivateKey::from_str(
            "4c0883a69102937d6231471b5dbb6204fe51296170827936ea5cce4b76994b0f",
        )
        .unwrap();
        let pub_key = signer.add_key(private_key).unwrap();

        let router_address = Address([0x01; 20]);
        let commitments = [MyComm([1, 2]), MyComm([3, 4])];

        let commitments_digest = commitments.to_digest();
        let signature =
            sign_commitments_digest(commitments_digest, &signer, pub_key, router_address).unwrap();
        let recovered =
            recover_from_commitments_digest(commitments_digest, &signature, router_address)
                .unwrap();

        assert_eq!(recovered, pub_key.to_address());
    }

    #[test]
    fn test_aggregated_commitments() {
        let signer = Signer::tmp();

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

    #[test]
    fn test_multisigned_commitment_digests() {
        let signer = Signer::tmp();

        let private_key = PrivateKey([1; 32]);
        let pub_key = signer.add_key(private_key).unwrap();

        let router_address = Address([0x01; 20]);
        let commitments = [MyComm([1, 2]), MyComm([3, 4])];
        let digests: IndexSet<_> = commitments.map(|c| c.to_digest()).into_iter().collect();

        let mut multisigned = MultisignedCommitmentDigests::new(digests.clone()).unwrap();
        assert_eq!(multisigned.digests(), &digests);
        assert_eq!(multisigned.signatures().len(), 0);

        let commitments_digest = commitments.to_digest();
        let signature =
            sign_commitments_digest(commitments_digest, &signer, pub_key, router_address).unwrap();

        multisigned
            .append_signature_with_check(commitments_digest, signature, router_address, |_| Ok(()))
            .unwrap();
        assert_eq!(multisigned.digests(), &digests);
        assert_eq!(multisigned.signatures().len(), 1);
    }

    #[test]
    fn test_multisigned_commitments() {
        let signer = Signer::tmp();

        let private_key = PrivateKey([1; 32]);
        let pub_key = signer.add_key(private_key).unwrap();

        let router_address = Address([1; 20]);
        let commitments = [MyComm([1, 2]), MyComm([3, 4])];
        let digests = commitments.map(|c| c.to_digest());
        let mut commitments_map: BTreeMap<_, _> = commitments
            .into_iter()
            .map(|c| (c.to_digest(), c))
            .collect();

        let mut multisigned =
            MultisignedCommitmentDigests::new(digests.into_iter().collect()).unwrap();
        let commitments_digest = commitments.to_digest();
        let signature =
            sign_commitments_digest(commitments_digest, &signer, pub_key, router_address).unwrap();

        multisigned
            .append_signature_with_check(commitments_digest, signature, router_address, |_| Ok(()))
            .unwrap();

        let multisigned_commitments =
            MultisignedCommitments::from_multisigned_digests(multisigned, |d| {
                commitments_map.remove(&d).unwrap()
            });

        assert_eq!(multisigned_commitments.commitments(), commitments);

        let parts = multisigned_commitments.into_parts();
        assert_eq!(parts.0.as_slice(), commitments.as_slice());
        assert_eq!(parts.1.len(), 1);
        parts.1.into_iter().for_each(|(k, v)| {
            assert_eq!(k, pub_key.to_address());
            assert_eq!(v, signature);
        });
    }
}
