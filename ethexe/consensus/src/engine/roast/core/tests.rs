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

use super::{ParticipantAction, ParticipantConfig, ParticipantEvent, RoastParticipant};
use crate::{
    engine::{
        dkg::{DkgConfig, DkgProtocol, FinalizeResult},
        roast::core::{participant::ParticipantState, tweak_public_key_package},
    },
    test_utils::ValidatorNetwork,
};
use ethexe_common::{
    Address,
    crypto::{
        DkgIdentifier, DkgKeyPackage, DkgSessionId, SignKind, SignNoncePackage, SignSessionRequest,
        tweak::hash_to_scalar,
    },
    k256::{EncodedPoint, FieldBytes},
};
use gprimitives::{ActorId, H256};
use roast_secp256k1_evm::frost::{Signature, VerifyingKey};
use std::collections::BTreeMap;

#[test]
fn signing_package_ignored_when_idle() {
    let self_address = Address::from([1; 20]);
    let mut participant = RoastParticipant::new(ParticipantConfig { self_address });
    let package = SignNoncePackage {
        session: DkgSessionId { era: 1 },
        msg_hash: H256([9; 32]),
        commitments: vec![(self_address, vec![1, 2, 3])],
    };

    let actions = participant
        .process_event(ParticipantEvent::SigningPackage(package))
        .expect("process signing package");

    assert!(actions.is_empty());
    assert!(matches!(participant.state(), ParticipantState::Idle));
}

/// Runs an in-memory DKG to build identifiers and key packages for tests.
fn build_dkg_materials(
    participants: &[Address],
    session: DkgSessionId,
    threshold: u16,
) -> (
    BTreeMap<Address, DkgIdentifier>,
    BTreeMap<Address, DkgKeyPackage>,
) {
    let mut protocols: Vec<(Address, DkgProtocol)> = participants
        .iter()
        .map(|address| {
            (
                *address,
                DkgProtocol::new(DkgConfig {
                    session,
                    threshold,
                    participants: participants.to_vec(),
                    self_address: *address,
                })
                .expect("protocol init"),
            )
        })
        .collect();

    let mut round1_messages = Vec::new();
    for (address, protocol) in protocols.iter_mut() {
        let round1 = protocol.generate_round1().expect("round1");
        round1_messages.push((*address, round1));
    }

    for (_, protocol) in protocols.iter_mut() {
        for (from, message) in &round1_messages {
            protocol
                .receive_round1(*from, message.clone())
                .expect("receive round1");
        }
    }

    let mut round2_messages = Vec::new();
    for (address, protocol) in protocols.iter_mut() {
        let round2 = protocol.generate_round2().expect("round2");
        round2_messages.push((*address, round2));
    }

    for (_, protocol) in protocols.iter_mut() {
        for (from, message) in &round2_messages {
            protocol
                .receive_round2(*from, message.clone())
                .expect("receive round2");
        }
    }

    let mut identifiers = BTreeMap::new();
    let mut key_packages = BTreeMap::new();
    for (address, protocol) in protocols.iter_mut() {
        let identifier = protocol.identifier_for(*address).expect("identifier");
        identifiers.insert(*address, identifier);

        let key_package = match protocol.finalize().expect("finalize") {
            FinalizeResult::Completed { key_package, .. } => *key_package,
            other => panic!("unexpected finalize result: {other:?}"),
        };
        key_packages.insert(*address, key_package);
    }

    (identifiers, key_packages)
}

#[test]
fn participant_signs_after_request_and_package() {
    let participants = vec![
        Address::from([1; 20]),
        Address::from([2; 20]),
        Address::from([3; 20]),
    ];
    let self_address = participants[0];
    let session = DkgSessionId { era: 1 };
    let threshold = 2;
    let (identifiers, key_packages) = build_dkg_materials(&participants, session, threshold);

    let request = SignSessionRequest {
        session,
        leader: self_address,
        attempt: 0,
        msg_hash: H256([7; 32]),
        tweak_target: ActorId::from([9; 32]),
        threshold,
        participants: participants.clone(),
        kind: SignKind::BatchCommitment,
    };

    let mut participant = RoastParticipant::new(ParticipantConfig { self_address });
    let actions = participant
        .process_event(ParticipantEvent::SignRequest {
            request: request.clone(),
            key_package: Box::new(
                key_packages
                    .get(&self_address)
                    .expect("key package")
                    .clone(),
            ),
            identifiers: identifiers.clone(),
            pre_nonce: None,
        })
        .expect("sign request");

    let commit = match &actions[..] {
        [ParticipantAction::SendNonceCommit(commit)] => commit.clone(),
        other => panic!("unexpected actions: {other:?}"),
    };

    assert!(matches!(participant.state(), ParticipantState::NonceSent));

    let commitments = participants
        .iter()
        .map(|address| (*address, commit.nonce_commit.clone()))
        .collect::<Vec<_>>();
    let package = SignNoncePackage {
        session,
        msg_hash: request.msg_hash,
        commitments,
    };

    let actions = participant
        .process_event(ParticipantEvent::SigningPackage(package))
        .expect("signing package");

    match &actions[..] {
        [ParticipantAction::SendPartialSignature(partial)] => {
            assert_eq!(partial.session, session);
            assert_eq!(partial.from, self_address);
            assert_eq!(partial.msg_hash, request.msg_hash);
        }
        other => panic!("unexpected actions: {other:?}"),
    }

    assert!(matches!(participant.state(), ParticipantState::PartialSent));
}

#[test]
fn roast_signature_verifies_with_tweak() {
    let mut network = ValidatorNetwork::new(4);
    let era = 1;
    let threshold = 3;
    let participants = network.run_dkg(era, threshold, 64).expect("run dkg");
    network.assert_dkg_completed(era);

    let coordinator = network.coordinator_address();
    let msg_hash = H256([7; 32]);
    let tweak_target = ActorId::from([9; 32]);

    let aggregate = network
        .run_roast_signing(
            coordinator,
            msg_hash,
            era,
            tweak_target,
            threshold,
            participants,
            128,
        )
        .expect("run roast signing");

    let public_key_package = network
        .public_key_packages(era)
        .into_iter()
        .next()
        .expect("public key package");
    let tweaked_package =
        tweak_public_key_package(&public_key_package, hash_to_scalar(tweak_target))
            .expect("tweak public key package");
    let tweaked_pk: [u8; 33] = tweaked_package
        .verifying_key()
        .serialize()
        .expect("serialize tweaked verifying key")
        .as_slice()
        .try_into()
        .expect("tweaked verifying key size");

    assert_eq!(aggregate.tweaked_pk, tweaked_pk);

    let (r_x, r_y, z) = aggregate.signature_components();
    let r_point =
        EncodedPoint::from_affine_coordinates(&FieldBytes::from(r_x), &FieldBytes::from(r_y), true);
    let mut signature_bytes = [0u8; 65];
    signature_bytes[..33].copy_from_slice(r_point.as_bytes());
    signature_bytes[33..].copy_from_slice(&z);

    let signature = Signature::deserialize(&signature_bytes).expect("deserialize signature");
    let verifying_key =
        VerifyingKey::deserialize(&aggregate.tweaked_pk).expect("deserialize verifying key");

    verifying_key
        .verify(msg_hash.as_bytes(), &signature)
        .expect("verify aggregate signature");

    let bad_tweak_target = ActorId::from([8; 32]);
    let bad_tweaked_package =
        tweak_public_key_package(&public_key_package, hash_to_scalar(bad_tweak_target))
            .expect("tweak public key package (bad)");
    let bad_tweaked_pk: [u8; 33] = bad_tweaked_package
        .verifying_key()
        .serialize()
        .expect("serialize bad tweaked verifying key")
        .as_slice()
        .try_into()
        .expect("bad tweaked verifying key size");
    let bad_verifying_key =
        VerifyingKey::deserialize(&bad_tweaked_pk).expect("deserialize bad verifying key");
    assert!(
        bad_verifying_key
            .verify(msg_hash.as_bytes(), &signature)
            .is_err(),
        "signature must not verify with a different tweak"
    );

    let wrong_msg_hash = H256([8; 32]);
    assert!(
        verifying_key
            .verify(wrong_msg_hash.as_bytes(), &signature)
            .is_err(),
        "signature must not verify for a different message"
    );
}
