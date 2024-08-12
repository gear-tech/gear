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

use std::collections::{BTreeMap, BTreeSet};

use anyhow::Result;
use ethexe_signer::{Address, AsDigest, Digest, PublicKey, Signature, Signer};
use parity_scale_codec::{Decode, Encode};

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
        let signature =
            sign_commitments_digest(commitments.as_digest(), signer, pub_key, router_address)?;

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

pub(crate) struct MultisignedCommitmentDigests {
    digest: Digest,
    digests: Vec<Digest>,
    signatures: BTreeMap<Address, Signature>,
}

impl MultisignedCommitmentDigests {
    pub fn new(digests: BTreeSet<Digest>) -> Self {
        let digests: Vec<_> = digests.into_iter().collect();
        Self {
            digest: digests.as_digest(),
            digests,
            signatures: BTreeMap::new(),
        }
    }

    pub fn append_signature_with_check(
        &mut self,
        digest: Digest,
        signature: Signature,
        router_address: Address,
        check_origin: impl FnOnce(Address) -> Result<()>,
    ) -> Result<()> {
        if self.digest != digest {
            return Err(anyhow::anyhow!("Aggregated commitments digest mismatch"));
        }

        let origin = recover_from_commitments_digest(digest, &signature, router_address)?;
        check_origin(origin)?;

        self.signatures.insert(origin, signature);

        Ok(())
    }

    pub fn digests(&self) -> &[Digest] {
        self.digests.as_slice()
    }

    pub fn signatures(&self) -> &BTreeMap<Address, Signature> {
        &self.signatures
    }
}

pub(crate) struct MultisignedCommitments<C: AsDigest> {
    commitments: Vec<C>,
    signatures: BTreeMap<Address, Signature>,
}

impl<C: AsDigest> MultisignedCommitments<C> {
    pub fn from_multisigned_digests(
        multisigned: MultisignedCommitmentDigests,
        mut get_commitment: impl FnMut(Digest) -> Option<C>,
    ) -> Result<Self> {
        let MultisignedCommitmentDigests {
            digests,
            signatures,
            ..
        } = multisigned;

        let mut commitments = Vec::new();
        for digest in digests {
            let commitment = get_commitment(digest)
                .ok_or_else(|| anyhow::anyhow!("Missing commitment for {digest}"))?;
            commitments.push(commitment);
        }

        Ok(Self {
            commitments,
            signatures,
        })
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
    signer.sign_digest(pub_key, digest)
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
        let signer = Signer::tmp();

        let private_key = PrivateKey::from_str(
            "4c0883a69102937d6231471b5dbb6204fe51296170827936ea5cce4b76994b0f",
        )
        .unwrap();
        let pub_key = signer.add_key(private_key).unwrap();

        let router_address = Address([0x01; 20]);
        let commitments = vec![MyComm([1, 2]), MyComm([3, 4])];

        let commitments_digest = commitments.as_digest();
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
}
