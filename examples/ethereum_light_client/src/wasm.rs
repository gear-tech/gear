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
use gstd::{
    codec::{Decode, Encode},
    msg,
    prelude::*,
    ActorId,
};
use hex_literal::hex;
use ssz_rs::{Deserialize, Merkleized, Node};
use ark_serialize::CanonicalSerialize;

#[derive(Debug, Default)]
struct LightClientStore {
    finalized_header: Header,
    current_sync_committee: Vec<G1>,
    next_sync_committee: Option<SyncCommittee>,
    optimistic_header: Header,
    previous_max_active_participants: u64,
    current_max_active_participants: u64,
}

static mut LAST_CHECKPOINT: Option<Bytes32> = None;
static mut STORE: Option<LightClientStore> = None;

// type ArkScale<T> = ark_scale::ArkScale<T, { ark_scale::HOST_CALL }>;

// const BUILTIN_BLS381: ActorId = ActorId::new(hex!(
//     "6b6e292c382945e80bf51af2ba7fe9f458dcff81ae6075c46f9095e1bbecdc37"
// ));

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
    current_sync_committee
        .pubkeys
        .as_ref()
        .iter()
        .zip(init_msg.pub_keys.0.iter())
        .for_each(|(pub_key_compressed, pub_key)| {
            buffer.clear();
            <G1 as CanonicalSerialize>::serialize_compressed(&pub_key, &mut buffer).unwrap();
            assert_eq!(pub_key_compressed.as_ref(), &buffer[..]);
        });

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
        next_sync_committee_branch,
        finality_branch,
    } = msg;

    let update = Update::deserialize(&update[..]).expect("Unable to deserialize Update");
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
