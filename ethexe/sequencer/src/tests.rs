// This file is part of Gear.
//
// Copyright (C) 2024-2025 Gear Technologies Inc.
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

use super::*;
use ethexe_signer::{sha3, PrivateKey};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TestComm([u8; 2]);

impl ToDigest for TestComm {
    fn update_hasher(&self, hasher: &mut sha3::Keccak256) {
        sha3::Digest::update(hasher, self.0);
    }
}

#[test]
fn test_receive_commitments() {
    let signer = Signer::tmp();

    let router_address = Address([1; 20]);

    let validators_private_keys = [PrivateKey([1; 32]), PrivateKey([2; 32])];
    let validators: HashSet<_> = validators_private_keys
        .iter()
        .cloned()
        .map(|key| signer.add_key(key).unwrap().to_address())
        .collect();

    let validator1_private_key = validators_private_keys[0];
    let validator1_pub_key = PublicKey::from(validator1_private_key);
    let validator1 = validator1_pub_key.to_address();

    let commitments = [TestComm([0, 1]), TestComm([2, 3])];
    let aggregated = AggregatedCommitments::aggregate_commitments(
        commitments.to_vec(),
        &signer,
        validator1_pub_key,
        router_address,
    )
    .unwrap();

    let mut expected_commitments_storage = CommitmentsMap::new();
    let mut commitments_storage = CommitmentsMap::new();

    let private_key = PrivateKey([3; 32]);
    let pub_key = signer.add_key(private_key).unwrap();
    let incorrect_aggregated = AggregatedCommitments::aggregate_commitments(
        commitments.to_vec(),
        &signer,
        pub_key,
        router_address,
    )
    .unwrap();
    SequencerService::receive_commitments(
        incorrect_aggregated,
        &validators,
        router_address,
        &mut commitments_storage,
        |_| true,
    )
    .expect_err("Signature verification must fail");

    SequencerService::receive_commitments(
        aggregated.clone(),
        &validators,
        router_address,
        &mut commitments_storage,
        |_| true,
    )
    .unwrap();

    expected_commitments_storage.insert(
        commitments[0].to_digest(),
        CommitmentAndOrigins {
            commitment: commitments[0],
            origins: [validator1].iter().cloned().collect(),
        },
    );
    expected_commitments_storage.insert(
        commitments[1].to_digest(),
        CommitmentAndOrigins {
            commitment: commitments[1],
            origins: [validator1].iter().cloned().collect(),
        },
    );
    assert_eq!(expected_commitments_storage, commitments_storage);

    let validator2_private_key = validators_private_keys[1];
    let validator2_pub_key = PublicKey::from(validator2_private_key);
    let validator2 = validator2_pub_key.to_address();

    let aggregated = AggregatedCommitments::aggregate_commitments(
        commitments.to_vec(),
        &signer,
        validator2_pub_key,
        router_address,
    )
    .unwrap();

    SequencerService::receive_commitments(
        aggregated,
        &validators,
        router_address,
        &mut commitments_storage,
        |_| true,
    )
    .unwrap();

    expected_commitments_storage
        .get_mut(&commitments[0].to_digest())
        .unwrap()
        .origins
        .insert(validator2);
    expected_commitments_storage
        .get_mut(&commitments[1].to_digest())
        .unwrap()
        .origins
        .insert(validator2);
    assert_eq!(expected_commitments_storage, commitments_storage);
}

#[test]
fn test_blocks_commitment_candidate() {
    let threshold = 2;

    let mut commitments = BTreeMap::new();

    let commitment1 = BlockCommitment {
        hash: H256::random(),
        timestamp: rand::random(),
        previous_committed_block: H256::random(),
        predecessor_block: H256::random(),
        transitions: Default::default(),
    };
    let commitment2 = BlockCommitment {
        hash: H256::random(),
        timestamp: rand::random(),
        previous_committed_block: commitment1.hash,
        predecessor_block: H256::random(),
        transitions: Default::default(),
    };
    let commitment3 = BlockCommitment {
        hash: H256::random(),
        timestamp: rand::random(),
        previous_committed_block: commitment1.hash,
        predecessor_block: H256::random(),
        transitions: Default::default(),
    };

    let mut expected_digests = IndexSet::new();

    let candidate =
        SequencerService::blocks_commitment_candidate(&commitments, commitment1.hash, threshold);
    assert!(candidate.is_none());

    commitments.insert(
        commitment1.to_digest(),
        CommitmentAndOrigins {
            commitment: commitment1.clone(),
            origins: Default::default(),
        },
    );
    let candidate =
        SequencerService::blocks_commitment_candidate(&commitments, H256::random(), threshold);
    assert!(candidate.is_none());

    let candidate =
        SequencerService::blocks_commitment_candidate(&commitments, commitment1.hash, 0)
            .expect("Must have candidate");
    expected_digests.insert(commitment1.to_digest());
    assert_eq!(candidate.digests(), &expected_digests);

    commitments
        .get_mut(&commitment1.to_digest())
        .unwrap()
        .origins
        .extend([Address([1; 20]), Address([2; 20])]);
    commitments.insert(
        commitment2.to_digest(),
        CommitmentAndOrigins {
            commitment: commitment2.clone(),
            origins: [[1; 20], [2; 20]].map(Address).iter().cloned().collect(),
        },
    );
    commitments.insert(
        commitment3.to_digest(),
        CommitmentAndOrigins {
            commitment: commitment3.clone(),
            origins: [[1; 20], [2; 20]].map(Address).iter().cloned().collect(),
        },
    );

    let candidate =
        SequencerService::blocks_commitment_candidate(&commitments, commitment1.hash, threshold)
            .expect("Must have candidate");
    assert_eq!(candidate.digests(), &expected_digests);

    let candidate =
        SequencerService::blocks_commitment_candidate(&commitments, commitment2.hash, threshold)
            .expect("Must have candidate");
    expected_digests.insert(commitment2.to_digest());
    assert_eq!(candidate.digests(), &expected_digests);

    let candidate =
        SequencerService::blocks_commitment_candidate(&commitments, commitment3.hash, threshold)
            .expect("Must have candidate");
    expected_digests.pop();
    expected_digests.insert(commitment3.to_digest());
    assert_eq!(candidate.digests(), &expected_digests);
}

#[test]
fn test_codes_commitment_candidate() {
    let threshold = 2;

    let mut commitments = BTreeMap::new();

    let commitment1 = CodeCommitment {
        id: H256::random().0.into(),
        timestamp: 41,
        valid: true,
    };
    let commitment2 = CodeCommitment {
        id: H256::random().0.into(),
        timestamp: 42,
        valid: true,
    };
    let commitment3 = CodeCommitment {
        id: H256::random().0.into(),
        timestamp: 43,
        valid: false,
    };

    let candidate = SequencerService::codes_commitment_candidate(commitments.iter(), threshold);
    assert!(candidate.is_none());

    commitments.insert(
        commitment1.to_digest(),
        CommitmentAndOrigins {
            commitment: commitment1.clone(),
            origins: Default::default(),
        },
    );
    let candidate = SequencerService::codes_commitment_candidate(commitments.iter(), threshold);
    assert!(candidate.is_none());

    commitments
        .get_mut(&commitment1.to_digest())
        .unwrap()
        .origins
        .insert(Address([1; 20]));
    let candidate = SequencerService::codes_commitment_candidate(commitments.iter(), threshold);
    assert!(candidate.is_none());

    commitments
        .get_mut(&commitment1.to_digest())
        .unwrap()
        .origins
        .insert(Address([2; 20]));
    let candidate = SequencerService::codes_commitment_candidate(commitments.iter(), threshold)
        .expect("Must have candidate");
    let expected_digests: IndexSet<_> = [commitment1.to_digest()].into_iter().collect();
    assert_eq!(candidate.digests(), &expected_digests);
    // assert!(candidate.signatures().is_empty());

    commitments.insert(
        commitment2.to_digest(),
        CommitmentAndOrigins {
            commitment: commitment2.clone(),
            origins: [Address([3; 20]), Address([4; 20])]
                .iter()
                .cloned()
                .collect(),
        },
    );
    let candidate = SequencerService::codes_commitment_candidate(commitments.iter(), threshold)
        .expect("Must have candidate");
    let mut expected_digests: IndexSet<_> = [commitment1.to_digest(), commitment2.to_digest()]
        .into_iter()
        .collect();
    expected_digests.sort();
    assert_eq!(candidate.digests(), &expected_digests);
    // assert!(candidate.signatures().is_empty());

    commitments.insert(
        commitment3.to_digest(),
        CommitmentAndOrigins {
            commitment: commitment3,
            origins: [Address([5; 20])].iter().cloned().collect(),
        },
    );
    let candidate = SequencerService::codes_commitment_candidate(commitments.iter(), threshold)
        .expect("Must have candidate");
    assert_eq!(candidate.digests(), &expected_digests);
    // assert!(candidate.signatures().is_empty());
}

/*#[test]
#[should_panic(expected = "Guarantied by `Sequencer` implementation to be in the map")]
fn test_process_multisigned_candidate_empty_map() {
    let candidate =
        MultisignedCommitmentDigests::new([[2; 32]].map(Into::into).into_iter().collect()).unwrap();
    SequencerService::process_multisigned_candidate::<TestComm>(
        &mut Some(candidate),
        &mut Default::default(),
        0,
    );
}*/

/*#[test]
fn test_process_multisigned_candidate() {
    let signer = Signer::tmp();

    // Test candidate is None
    assert!(SequencerService::process_multisigned_candidate::<TestComm>(
        &mut None,
        &mut Default::default(),
        0
    )
    .is_none());

    // Test not enough signatures
    let mut candidate = Some(
        MultisignedCommitmentDigests::new([b"gear".to_digest()].into_iter().collect()).unwrap(),
    );
    assert!(SequencerService::process_multisigned_candidate(
        &mut candidate,
        &mut CommitmentsMap::<TestComm>::new(),
        2
    )
    .is_none());

    let validators_private_keys = [PrivateKey([1; 32]), PrivateKey([2; 32])];
    let validators_pub_keys = validators_private_keys.map(|key| signer.add_key(key).unwrap());
    let origins: BTreeSet<_> = validators_pub_keys
        .map(|k| k.to_address())
        .into_iter()
        .collect();

    let commitments = [TestComm([0, 1]), TestComm([2, 3]), TestComm([4, 5])];
    let mut commitments_map = commitments
        .iter()
        .map(|commitment| {
            (
                commitment.to_digest(),
                CommitmentAndOrigins {
                    commitment: *commitment,
                    origins: origins.clone(),
                },
            )
        })
        .collect();

    let mut candidate =
        MultisignedCommitmentDigests::new(commitments.iter().map(|c| c.to_digest()).collect())
            .unwrap();

    let router_address = Address([1; 20]);
    validators_pub_keys.iter().for_each(|pub_key| {
        let commitments_digest = commitments.to_digest();
        candidate
            .append_signature_with_check(
                commitments_digest,
                agro::sign_commitments_digest(
                    commitments_digest,
                    &signer,
                    *pub_key,
                    router_address,
                )
                .unwrap(),
                router_address,
                |_| Ok(()),
            )
            .unwrap();
    });

    let mut candidate = Some(candidate);

    assert!(SequencerService::process_multisigned_candidate(
        &mut candidate,
        &mut commitments_map,
        2
    )
    .is_some(),);
    assert!(commitments_map.is_empty());
    assert!(candidate.is_none());
}*/
