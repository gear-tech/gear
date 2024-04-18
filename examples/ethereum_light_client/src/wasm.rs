// This file is part of Gear.

// Copyright (C) 2024 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use super::*;
use core::ops::Neg;
use gbuiltin_bls381::*;
use gstd::{
    codec::{Decode, Encode},
    debug,
    msg,
    prelude::*,
    ActorId,
};
use hex_literal::hex;
use ssz_rs::{Deserialize, Merkleized, Node, Bitvector, Vector};
use ark_bls12_381::{Bls12_381, G1Affine, G2Affine};
use ark_ec::{
    hashing::{map_to_curve_hasher::MapToCurveBasedHasher, HashToCurve, curve_maps::wb::WBConfig},
    bls12::Bls12Config,
    pairing::Pairing,
    CurveGroup, Group, AffineRepr,
};
use ark_ff::{fields::field_hashers::DefaultFieldHasher, Zero, Field};
use ark_serialize::CanonicalSerialize;

type WBMap = ark_ec::hashing::curve_maps::wb::WBMap<<ark_bls12_381::Config as Bls12Config>::G2Config>;

#[derive(Debug, Default)]
struct LightClientStore {
    finalized_header: Header,
    current_sync_committee: Vec<G1>,
    next_sync_committee: Option<Vec<G1>>,
    optimistic_header: Header,
    previous_max_active_participants: u64,
    current_max_active_participants: u64,
}

static mut LAST_CHECKPOINT: Option<Bytes32> = None;
static mut STORE: Option<LightClientStore> = None;

const BUILTIN_BLS381: ActorId = ActorId::new(hex!(
    "6b6e292c382945e80bf51af2ba7fe9f458dcff81ae6075c46f9095e1bbecdc37"
));

fn get_bits(bitfield: &Bitvector<512>) -> u64 {
    bitfield
        .iter()
        .fold(0u64, |sum, current| sum + u64::from(*current))
}

fn get_participating_keys(
    committee: &[G1],
    bitfield: &Bitvector<512>,
) -> Vec<G1> {
    assert_eq!(committee.len(), 512);

    bitfield.iter().zip(committee.iter())
        .filter_map(|(bit, pub_key)| {
            bit.then_some(*pub_key)
        })
        .collect::<Vec<_>>()
}

pub fn branch_to_nodes(branch: Vec<Bytes32>) -> Vec<Node> {
    branch
        .iter()
        .map(bytes32_to_node)
        .collect::<Vec<Node>>()
}

pub fn bytes32_to_node(bytes: &Bytes32) -> Node {
    Node::try_from(bytes.as_slice()).unwrap()
}

pub fn is_proof_valid<L: Merkleized>(
    attested_header: &Header,
    leaf_object: &mut L,
    branch: &[Bytes32],
    depth: usize,
    index: usize,
) -> bool {
    let leaf_hash = match leaf_object.hash_tree_root() {
        Ok(x) => x,
        _ => return false,
    };

    let state_root = bytes32_to_node(&attested_header.state_root);
    let branch = branch_to_nodes(branch.to_vec());
    
    ssz_rs::is_valid_merkle_branch(&leaf_hash, branch.iter(), depth, index, &state_root)
}

fn is_current_committee_proof_valid(
    attested_header: &Header,
    current_committee: &mut SyncCommittee,
    current_committee_branch: &[Bytes32],
) -> bool {
    is_proof_valid(
        attested_header,
        current_committee,
        current_committee_branch,
        5,
        22,
    )
}

fn is_finality_proof_valid(
    attested_header: &Header,
    finality_header: &mut Header,
    finality_branch: &[Bytes32],
) -> bool {
    is_proof_valid(attested_header, finality_header, finality_branch, 6, 41)
}

fn is_next_committee_proof_valid(
    attested_header: &Header,
    next_committee: &mut SyncCommittee,
    next_committee_branch: &[Bytes32],
) -> bool {
    is_proof_valid(
        attested_header,
        next_committee,
        next_committee_branch,
        5,
        23,
    )
}

#[derive(SimpleSerialize, Default, Debug)]
struct SigningData {
    object_root: Bytes32,
    domain: Bytes32,
}

#[derive(SimpleSerialize, Default, Debug)]
struct ForkData {
    current_version: Vector<u8, 4>,
    genesis_validator_root: Bytes32,
}

pub fn compute_signing_root(object_root: Bytes32, domain: Bytes32) -> Node {
    let mut data = SigningData {
        object_root,
        domain,
    };

    data.hash_tree_root().unwrap()
}

pub fn compute_domain(
    domain_type: &[u8],
    fork_version: Vector<u8, 4>,
    genesis_root: Bytes32,
) -> Bytes32 {
    let fork_data_root = compute_fork_data_root(fork_version, genesis_root);
    let start = domain_type;
    let end = &fork_data_root.as_ref()[..28];
    let d = [start, end].concat();

    d.to_vec().try_into().unwrap()
}

fn compute_fork_data_root(
    current_version: Vector<u8, 4>,
    genesis_validator_root: Bytes32,
) -> Node {
    let mut fork_data = ForkData {
        current_version,
        genesis_validator_root,
    };

    fork_data.hash_tree_root().unwrap()
}

fn compute_committee_sign_root(header: Bytes32, _slot: u64) -> Node {
    // Sepolia = 0xd8ea171f3c94aea21ebc42a1ed61052acf3f9209c00e4efbaaddac09ed9b8078
    let genesis_root = [216, 234, 23, 31, 60, 148, 174, 162, 30, 188, 66, 161, 237, 97, 5, 42, 207, 63, 146, 9, 192, 14, 78, 251, 170, 221, 172, 9, 237, 155, 128, 120];
    let genesis_root = genesis_root.as_ref().try_into().unwrap();

    // let domain_type = &hex::decode("07000000")?[..];
    // 0x07000000
    let domain_type = [0x07, 0x00, 0x00, 0x00];

    // let fork_version =
    //     Vector::try_from(self.config.fork_version(slot)).map_err(|(_, err)| err)?;
    // Deneb = 0x90000073
    let fork_version = vec![0x90, 0x00, 0x00, 0x73];
    let fork_version = fork_version.try_into().unwrap();
    let domain = compute_domain(&domain_type, fork_version, genesis_root);

    compute_signing_root(header, domain)
}

async fn verify_sync_committee_signture(
    pub_keys: &[G1],
    mut attested_header: Header,
    signature: &G2,
    signature_slot: u64,
) -> bool {
    let header_root =
            Bytes32::try_from(attested_header.hash_tree_root().unwrap().as_ref()).unwrap();
    let signing_root = compute_committee_sign_root(header_root, signature_slot);
    debug!("signing_root = {:?}", signing_root.as_ref());

    let pub_key_aggregated = pub_keys
        .iter()
        .skip(1)
        .fold(pub_keys[0], |pub_key_aggregated, pub_key| pub_key_aggregated + *pub_key);

    // Ensure AggregatePublicKey is not at infinity
    if pub_key_aggregated.is_zero() {
        return false;
    }

    /// Domain Separation Tag for signatures on G2
    pub const DST_G2: &[u8] = b"BLS_SIG_BLS12381G2_XMD:SHA-256_SSWU_RO_POP_";
    let mapper = MapToCurveBasedHasher::<G2, DefaultFieldHasher<sha2::Sha256>, WBMap>::new(DST_G2).unwrap();
    let message = mapper.hash(signing_root.as_ref()).unwrap();
    let message: G2Affine = message.into();

    let pub_key: G1Affine = From::from(pub_key_aggregated);
    let signature: G2Affine = From::from(*signature);
    let generator_g1_negative = G1Affine::generator().neg();

    // pairing
    let a: ArkScale<Vec<G1Affine>> = vec![generator_g1_negative, pub_key].into();
    let b: ArkScale<Vec<G2Affine>> = vec![signature, message].into();
    let request = Request::MultiMillerLoop { a: a.encode(), b: b.encode(), }.encode();
    let reply = msg::send_bytes_for_reply(BUILTIN_BLS381, &request, 0, 0)
        .expect("Failed to send message")
        .await
        .expect("Received error reply");
    let response = Response::decode(&mut reply.as_slice()).unwrap();
    let miller_loop = match response {
        Response::MultiMillerLoop(v) => v,
        _ => unreachable!(),
    };

    let request = Request::FinalExponentiation {
        f: miller_loop,
    }
    .encode();
    let reply = msg::send_bytes_for_reply(BUILTIN_BLS381, &request, 0, 0)
        .expect("Failed to send message")
        .await
        .expect("Received error reply");
    let response = Response::decode(&mut reply.as_slice()).unwrap();
    let exp = match response {
        Response::FinalExponentiation(v) => {
            ArkScale::<<Bls12_381 as Pairing>::TargetField>::decode(&mut v.as_slice())
                .unwrap()
        }
        _ => unreachable!(),
    };

    <Bls12_381 as Pairing>::TargetField::ONE == exp.0
}

#[no_mangle]
extern "C" fn init() {
    let init_msg: Init = msg::load().expect("Unable to decode `Init` message");

    let last_checkpoint = Bytes32::try_from(&init_msg.last_checkpoint[..]).expect("Unable to create Bytes32 from [u8; 32]");
    let mut finalized_header = Header::deserialize(&init_msg.finalized_header[..]).expect("Unable to deserialize finalized header");
    let header_hash = finalized_header.hash_tree_root().expect("Unable to calculate header hash");
    if header_hash.as_ref() != last_checkpoint.as_slice() {
        panic!("Header hash is not valid. Expected = {:?}, actual = {:?}.", last_checkpoint, header_hash);
    }

    let mut current_sync_committee = SyncCommittee::deserialize(&init_msg.current_sync_committee[..]).expect("Unable to deserialize current sync_committee");

    let mut buffer = Vec::with_capacity(512);
    let pub_key_count = current_sync_committee
        .pubkeys
        .as_ref()
        .iter()
        .zip(init_msg.pub_keys.0.iter())
        .fold(0, |count, (pub_key_compressed, pub_key)| {
            buffer.clear();
            <G1 as CanonicalSerialize>::serialize_compressed(&pub_key, &mut buffer).unwrap();
            assert_eq!(pub_key_compressed.as_ref(), &buffer[..]);

            count + 1
        });
    assert_eq!(pub_key_count, 512);

    // let _pub_key_aggregated = init_msg
    //     .pub_keys
    //     .0
    //     .iter()
    //     .skip(1)
    //     .fold(init_msg
    //         .pub_keys
    //         .0[0], |pub_key_aggregated, pub_key| pub_key_aggregated + *pub_key);

    let current_sync_committee_branch = init_msg
        .current_sync_committee_branch
        .iter()
        .map(|branch| Bytes32::try_from(&branch[..]).expect("Unable to create Bytes32 from [u8; 32]"))
        .collect::<Vec<_>>();
    if !is_current_committee_proof_valid(
        &finalized_header,
        &mut current_sync_committee,
        &current_sync_committee_branch,
    ) {
        panic!("Current sync committee proof is not valid.");
    }

    let optimistic_header = Header::deserialize(&init_msg.optimistic_header[..]).expect("Unable to deserialize optimistic header");
    unsafe {
        LAST_CHECKPOINT = Some(last_checkpoint);
        STORE = Some(LightClientStore {
            finalized_header,
            current_sync_committee: init_msg.pub_keys.0,
            next_sync_committee: None,
            optimistic_header,
            previous_max_active_participants: 0,
            current_max_active_participants: 0,
        });
    }
}

#[gstd::async_main]
async fn main() {
    let msg: Handle = msg::load().expect("Unable to decode `HandleMessage`");
    let Handle::Update {
        update,
        signature_slot,
        sync_committee_signature,
        next_sync_committee,
        next_sync_committee_branch,
        finality_branch,
    } = msg;

    let update = Update::deserialize(&update[..]).expect("Unable to deserialize Update");

    let bits = get_bits(&update.sync_aggregate.sync_committee_bits);
    if bits == 0 {
        debug!("ConsensusError::InsufficientParticipation");
        return;
    }

    let update_finalized_slot = update.finalized_header.clone().unwrap_or_default().slot;
    let valid_time = signature_slot > update.attested_header.slot.as_u64()
        && update.attested_header.slot >= update_finalized_slot;
    if !valid_time {
        debug!("ConsensusError::InvalidTimestamp.into()");
        return;
    }

    let store = unsafe { &STORE }.as_ref().unwrap();
    let store_period = calc_sync_period(store.finalized_header.slot.into());
    let update_sig_period = calc_sync_period(signature_slot);
    let valid_period = if store.next_sync_committee.is_some() {
        update_sig_period == store_period || update_sig_period == store_period + 1
    } else {
        update_sig_period == store_period
    };

    if !valid_period {
        debug!("ConsensusError::InvalidPeriod.into()");
        return;
    }

    let update_attested_period = calc_sync_period(update.attested_header.slot.into());
    let update_has_next_committee = store.next_sync_committee.is_none()
        && update.next_sync_committee.is_some()
        && next_sync_committee.is_some()
        && update_attested_period == store_period;

    if update.attested_header.slot <= store.finalized_header.slot
        && !update_has_next_committee
    {
        debug!("ConsensusError::NotRelevant.into()");
        return;
    }

    if update.finalized_header.is_some() && finality_branch.is_some() {
        let is_valid = is_finality_proof_valid(
            &update.attested_header,
            &mut update.finalized_header.clone().unwrap(),
            &finality_branch.clone().unwrap()
                .iter()
                .map(|branch| Bytes32::try_from(&branch[..]).expect("Unable to create Bytes32 from [u8; 32]"))
                .collect::<Vec<_>>(),
        );

        if !is_valid {
            debug!("ConsensusError::InvalidFinalityProof.into()");
            return;
        }
    }

    if update.next_sync_committee.is_some() && next_sync_committee_branch.is_some() {
        let is_valid = is_next_committee_proof_valid(
            &update.attested_header,
            &mut update.next_sync_committee.clone().unwrap(),
            &next_sync_committee_branch.clone().unwrap()
                .iter()
                .map(|branch| Bytes32::try_from(&branch[..]).expect("Unable to create Bytes32 from [u8; 32]"))
                .collect::<Vec<_>>(),
        );

        if !is_valid {
            debug!("ConsensusError::InvalidNextSyncCommitteeProof.into()");
            return;
        }
    }

    let sync_committee = match update_sig_period {
        period if period == store_period => &store.current_sync_committee,
        _ => store.next_sync_committee.as_ref().unwrap(),
    };

    let pub_keys =
        get_participating_keys(sync_committee, &update.sync_aggregate.sync_committee_bits);

    let is_valid_sig = verify_sync_committee_signture(
        &pub_keys,
        update.attested_header.clone(),
        &sync_committee_signature.0,
        signature_slot,
    ).await;

    debug!("is_valid_sig = {is_valid_sig}");

    // if !is_valid_sig {
    //     return Err(ConsensusError::InvalidSignature.into());
    // }

    // let contract = unsafe { CONTRACT.as_mut().expect("The contract is not initialized") };

    // match msg {
    //     HandleMessage::MillerLoop {
    //         message,
    //         signatures,
    //     } => {
    //         let aggregate_pub_key: ArkScale<Vec<G2Affine>> =
    //             vec![contract.aggregate_pub_key].into();

    //         let request = Request::MultiMillerLoop {
    //             a: message,
    //             b: aggregate_pub_key.encode(),
    //         }
    //         .encode();
    //         let reply = msg::send_bytes_for_reply(BUILTIN_BLS381, &request, 0, 0)
    //             .expect("Failed to send message")
    //             .await
    //             .expect("Received error reply");

    //         let response = Response::decode(&mut reply.as_slice()).unwrap();
    //         let miller_out1 = match response {
    //             Response::MultiMillerLoop(v) => v,
    //             _ => unreachable!(),
    //         };

    //         let mut aggregate_signature: G1Affine = Default::default();
    //         for signature in signatures.iter() {
    //             let signature = <ArkScale<<Bls12_381 as Pairing>::G1Affine> as Decode>::decode(
    //                 &mut signature.as_slice(),
    //             )
    //             .unwrap();
    //             aggregate_signature = (aggregate_signature + signature.0).into();
    //         }
    //         let aggregate_signature: ArkScale<Vec<G1Affine>> = vec![aggregate_signature].into();
    //         let g2_gen: ArkScale<Vec<G2Affine>> = vec![contract.g2_gen].into();
    //         let request = Request::MultiMillerLoop {
    //             a: aggregate_signature.encode(),
    //             b: g2_gen.encode(),
    //         }
    //         .encode();
    //         let reply = msg::send_bytes_for_reply(BUILTIN_BLS381, &request, 0, 0)
    //             .expect("Failed to send message")
    //             .await
    //             .expect("Received error reply");
    //         let response = Response::decode(&mut reply.as_slice()).unwrap();
    //         let miller_out2 = match response {
    //             Response::MultiMillerLoop(v) => v,
    //             _ => unreachable!(),
    //         };

    //         contract.miller_out = (Some(miller_out1), Some(miller_out2));
    //     }

    //     HandleMessage::Exp => {
    //         if let (Some(miller_out1), Some(miller_out2)) = &contract.miller_out {
    //             let request = Request::FinalExponentiation {
    //                 f: miller_out1.clone(),
    //             }
    //             .encode();
    //             let reply = msg::send_bytes_for_reply(BUILTIN_BLS381, &request, 0, 0)
    //                 .expect("Failed to send message")
    //                 .await
    //                 .expect("Received error reply");
    //             let response = Response::decode(&mut reply.as_slice()).unwrap();
    //             let exp1 = match response {
    //                 Response::FinalExponentiation(v) => {
    //                     ArkScale::<<Bls12_381 as Pairing>::TargetField>::decode(&mut v.as_slice())
    //                         .unwrap()
    //                 }
    //                 _ => unreachable!(),
    //             };

    //             let request = Request::FinalExponentiation {
    //                 f: miller_out2.clone(),
    //             }
    //             .encode();
    //             let reply = msg::send_bytes_for_reply(BUILTIN_BLS381, &request, 0, 0)
    //                 .expect("Failed to send message")
    //                 .await
    //                 .expect("Received error reply");
    //             let response = Response::decode(&mut reply.as_slice()).unwrap();
    //             let exp2 = match response {
    //                 Response::FinalExponentiation(v) => {
    //                     ArkScale::<<Bls12_381 as Pairing>::TargetField>::decode(&mut v.as_slice())
    //                         .unwrap()
    //                 }
    //                 _ => unreachable!(),
    //             };

    //             assert_eq!(exp1.0, exp2.0);

    //             contract.miller_out = (None, None);
    //         }
    //     }
    // }
}
