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
use core::{cmp, ops::Neg};
use gbuiltin_bls381::*;
use gstd::{
    codec::{Decode, Encode},
    debug,
    msg,
    prelude::*,
    ActorId,
};
use circular_buffer::CircularBuffer;
use hex_literal::hex;
use ssz_rs::{Deserialize, Merkleized, Node, Bitvector, Vector, List};
use ark_bls12_381::{Bls12_381, G1Affine, G2Affine};
use ark_ec::{
    hashing::{map_to_curve_hasher::MapToCurveBasedHasher, HashToCurve, curve_maps::wb::WBConfig},
    bls12::Bls12Config,
    pairing::Pairing,
    CurveGroup, Group, AffineRepr,
};
use ark_ff::{fields::field_hashers::DefaultFieldHasher, Zero, Field};
use ark_serialize::CanonicalSerialize;
use tree_hash::TreeHash;

type WBMap = ark_ec::hashing::curve_maps::wb::WBMap<<ark_bls12_381::Config as Bls12Config>::G2Config>;

#[derive(Debug, Default)]
struct LightClientStore {
    finalized_header: Header,
    current_sync_committee: Vec<G1>,
    next_sync_committee: Option<Vec<G1>>,
}

static mut LAST_CHECKPOINT: Option<Bytes32> = None;
static mut STORE: Option<LightClientStore> = None;
static mut BLOCKS: CircularBuffer<256, (ExecutionPayloadHeader, List<Node, 1_048_576>)> = CircularBuffer::new();

const BUILTIN_BLS381: ActorId = ActorId::new(hex!(
    "6b6e292c382945e80bf51af2ba7fe9f458dcff81ae6075c46f9095e1bbecdc37"
));

#[derive(Clone)]
struct RingSha256(ring::digest::Context);

impl Default for RingSha256 {
    fn default() -> Self {
        Self(ring::digest::Context::new(&ring::digest::SHA256))
    }
}

impl digest::DynDigest for RingSha256 {
    fn update(&mut self, data: &[u8]) {
        self.0.update(data)
    }

    fn finalize_into(self, buf: &mut [u8]) -> Result<(), digest::InvalidBufferSize> {
        if buf.len() != self.output_size() {
            return Err(digest::InvalidBufferSize);
        }

        buf.copy_from_slice(self.0.finish().as_ref());

        Ok(())
    }

    fn finalize_into_reset(
        &mut self,
        out: &mut [u8]
    ) -> Result<(), digest::InvalidBufferSize> {
        self
            .clone()
            .finalize_into(out)
            .map(|result| {
                self.reset();

                result
            })
    }

    fn reset(&mut self) {
        self.0 = ring::digest::Context::new(&ring::digest::SHA256);
    }

    fn output_size(&self) -> usize {
        32
    }

    fn box_clone(&self) -> Box<dyn digest::DynDigest> {
        Box::new(self.clone())
    }
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

fn is_valid_merkle_branch(
    leaf: [u8; 32],
    branch: &[[u8; 32]],
    depth: u32,
    index: u32,
    root: &[u8; 32],
) -> bool {
    use digest::DynDigest;

    let mut value = leaf;

    let mut hasher = RingSha256::default();
    let mut iter = branch.iter();
    for i in 0..depth {
        let Some(next_node) = iter.next() else {
            return false;
        };

        let (node_first, node_second) = match (index / 2u32.pow(i)) % 2 {
            0 => (value.as_ref(), next_node.as_ref()),
            _ => (next_node.as_ref(), value.as_ref()),
        };

        hasher.update(node_first);
        hasher.update(node_second);

        hasher.finalize_into_reset(&mut value).unwrap()
    }

    value == *root
}

pub fn is_proof_valid2(
    attested_header: &Header,
    leaf_hash: [u8; 32],
    branch: &[[u8; 32]],
    depth: u32,
    index: u32,
) -> bool {
    let state_root = <[u8; 32]>::try_from(attested_header.state_root.as_slice()).unwrap();

    is_valid_merkle_branch(leaf_hash, branch, depth, index, &state_root)
}

fn is_current_committee_proof_valid2(
    attested_header: &Header,
    current_committee: &SyncCommittee2,
    current_committee_branch: &[[u8; 32]],
) -> bool {
    let leaf_hash = current_committee.tree_hash_root();

    is_proof_valid2(
        attested_header,
        leaf_hash.0,
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
    next_committee: &SyncCommittee2,
    next_committee_branch: &[[u8; 32]],
) -> bool {
    let leaf_hash = next_committee.tree_hash_root();

    is_proof_valid2(
        attested_header,
        leaf_hash.0,
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
    pub_keys: Vec<G1>,
    mut attested_header: Header,
    signature: &G2,
    signature_slot: u64,
) -> bool {
    let header_root =
            Bytes32::try_from(attested_header.hash_tree_root().unwrap().as_ref()).unwrap();
    let signing_root = compute_committee_sign_root(header_root, signature_slot);
    debug!("signing_root = {:?}", signing_root.as_ref());

    // let pub_key_aggregated = pub_keys
    //     .iter()
    //     .skip(1)
    //     .fold(pub_keys[0], |pub_key_aggregated, pub_key| pub_key_aggregated + *pub_key);
    let points: ArkScale<Vec<G1>> = pub_keys.into();
    let request = Request::AggregateG1 {
        points: points.encode(),
    }
    .encode();
    let reply = msg::send_bytes_for_reply(BUILTIN_BLS381, &request, 0, 0)
        .expect("Failed to send message")
        .await
        .expect("Received error reply");
    let response = Response::decode(&mut reply.as_slice()).expect("Aggregate G1 reply should be properly encoded");
    let pub_key_aggregated = match response {
        Response::AggregateG1(v) => {
            ArkScale::<G1>::decode(&mut v.as_slice())
                .expect("Aggregate G1 result should properly encoded")
        }
        _ => unreachable!(),
    };

    // Ensure AggregatePublicKey is not at infinity
    if pub_key_aggregated.0.is_zero() {
        return false;
    }

    /// Domain Separation Tag for signatures on G2
    pub const DST_G2: &[u8] = b"BLS_SIG_BLS12381G2_XMD:SHA-256_SSWU_RO_POP_";
    // let mapper = MapToCurveBasedHasher::<G2, DefaultFieldHasher<sha2::Sha256>, WBMap>::new(DST_G2).unwrap();
    let mapper = MapToCurveBasedHasher::<G2, DefaultFieldHasher<RingSha256>, WBMap>::new(DST_G2).unwrap();
    let message = mapper.hash(signing_root.as_ref()).unwrap();
    let message: G2Affine = message.into();

    let pub_key: G1Affine = From::from(pub_key_aggregated.0);
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

    // let mut current_sync_committee = SyncCommittee::deserialize(&init_msg.current_sync_committee[..]).expect("Unable to deserialize current sync_committee");
    let current_sync_committee = init_msg.current_sync_committee;

    let mut buffer = Vec::with_capacity(512);
    let pub_key_count = current_sync_committee
        .pubkeys
        .0
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

    // let current_sync_committee_branch = init_msg
    //     .current_sync_committee_branch
    //     .iter()
    //     .map(|branch| Bytes32::try_from(&branch[..]).expect("Unable to create Bytes32 from [u8; 32]"))
    //     .collect::<Vec<_>>();
    if !is_current_committee_proof_valid2(
        &finalized_header,
        &current_sync_committee,
        &init_msg.current_sync_committee_branch,
    ) {
        panic!("Current sync committee proof is not valid.");
    }

    unsafe {
        LAST_CHECKPOINT = Some(last_checkpoint);
        STORE = Some(LightClientStore {
            finalized_header,
            current_sync_committee: init_msg.pub_keys.0,
            next_sync_committee: None,
        });
    }
}

#[gstd::async_main]
async fn main() {
    let msg: Handle = msg::load().expect("Unable to decode `HandleMessage`");
    match msg {
        Handle::Update {
            update,
            signature_slot,
            next_sync_committee,
            sync_committee_signature,
            next_sync_committee_keys,
            next_sync_committee_branch,
            finality_branch,
        } => handle_update(update, signature_slot, next_sync_committee, sync_committee_signature, next_sync_committee_keys, next_sync_committee_branch, finality_branch).await,

        Handle::BeaconBlockBody {
            beacon_block_body_light,
            transaction_hashes,
        } => handle_beacon_block_body(beacon_block_body_light, transaction_hashes).await,
    }
}

async fn handle_update(
    update: Vec<u8>,
    signature_slot: u64,
    next_sync_committee: Option<SyncCommittee2>,
    // serialized without compression
    sync_committee_signature: ArkScale<G2>,
    next_sync_committee_keys: Option<ArkScale<Vec<G1>>>,
    next_sync_committee_branch: Option<Vec<[u8; 32]>>,
    finality_branch: Vec<[u8; 32]>,
) {
    let update = Update::deserialize(&update[..]).expect("Unable to deserialize Update");

    let update_finalized_slot = update
        .finalized_header
        .slot
        .as_u64();
    let valid_time = signature_slot > update.attested_header.slot.as_u64()
        && update.attested_header.slot.as_u64() >= update_finalized_slot;
    if !valid_time {
        debug!("ConsensusError::InvalidTimestamp.into()");
        return;
    }

    let store = unsafe { STORE.as_mut() }.unwrap();
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

    let sync_committee = match update_sig_period {
        period if period == store_period => &store.current_sync_committee,
        _ => store.next_sync_committee.as_ref().unwrap(),
    };

    let pub_keys =
        get_participating_keys(sync_committee, &update.sync_aggregate.sync_committee_bits);
    let committee_count = pub_keys.len() as u64;
    // committee_count < 512 * 2 / 3
    if committee_count * 3 < 512 * 2 {
        debug!("skipping block with low vote count");
        return;
    }

    let update_finalized_period = calc_sync_period(update_finalized_slot);
    let update_is_newer = update_finalized_slot > store.finalized_header.slot.as_u64();
    let update_attested_period = calc_sync_period(update.attested_header.slot.into());
    let update_has_finalized_next_committee =
        // has sync update
        next_sync_committee_keys.is_some() && next_sync_committee_branch.is_some()
        && update_finalized_period == update_attested_period;

    if update_is_newer || update_has_finalized_next_committee {
        let is_valid_sig = verify_sync_committee_signture(
            pub_keys,
            update.attested_header.clone(),
            &sync_committee_signature.0,
            signature_slot,
        ).await;
    
        debug!("is_valid_sig = {is_valid_sig}");
        if !is_valid_sig {
            return;
        }
    }

    if update_is_newer {
        if is_finality_proof_valid(
            &update.attested_header,
            &mut update.finalized_header.clone(),
            &finality_branch
                .iter()
                .map(|branch| Bytes32::try_from(branch.as_ref()).expect("Unable to create Bytes32 from [u8; 32]"))
                .collect::<Vec<_>>(),
        ) {
            store.finalized_header = update.finalized_header.clone();

            if store.finalized_header.slot.as_u64() % SLOTS_PER_EPOCH == 0 {
                let checkpoint_res = store.finalized_header.hash_tree_root();
                if let Ok(checkpoint) = checkpoint_res {
                    unsafe { LAST_CHECKPOINT = Some(Bytes32::try_from(checkpoint.as_ref()).expect("Last checkpoint: unable to create Bytes32 from Vec")); }
                }
            }
        } else {
            debug!("ConsensusError::InvalidFinalityProof.into()");
        }
    }

    if !update_has_finalized_next_committee {
        return;
    }

    if next_sync_committee.is_some() && next_sync_committee_branch.is_some() {
        let is_valid = is_next_committee_proof_valid(
            &update.attested_header,
            &next_sync_committee.as_ref().unwrap(),
            &next_sync_committee_branch.as_ref().unwrap(),
        );

        if !is_valid {
            debug!("ConsensusError::InvalidNextSyncCommitteeProof.into()");
            return;
        }
    }

    match &store.next_sync_committee {
        Some(stored_next_sync_committee) if update_finalized_period == store_period + 1 => {
            debug!("sync committee updated");
            store.current_sync_committee = stored_next_sync_committee.clone();
            store.next_sync_committee = next_sync_committee_keys.clone().map(|ark_scale| ark_scale.0);
        }

        None => {
            store.next_sync_committee = next_sync_committee_keys.clone().map(|ark_scale| ark_scale.0);
        }

        _ => (),
    }
}

async fn handle_beacon_block_body(
    // ssz_rs serialized
    beacon_block_body_light: Vec<u8>,
    // ssz_rs serialized
    transaction_hashes: Vec<u8>,
) {
    let mut beacon_block_body_light = BeaconBlockBodyLight::deserialize(&beacon_block_body_light[..]).expect("Unable to deserialize BeaconBlockBodyLight");
    let blocks = unsafe { &mut BLOCKS };
    if blocks
        .iter()
        .find(|(execution_payload_header, _)| execution_payload_header.block_number() == beacon_block_body_light.execution_payload_header().block_number())
        .is_some()
    {
        debug!("already contains the block. Skipping");
        return;
    }

    let store = unsafe { STORE.as_mut() }.unwrap();

    let block_hash = beacon_block_body_light.hash_tree_root().expect("Unable to calculate hash of beacon block body");
    debug!("store.finalized_header.slot = {:?}", store.finalized_header.slot);
    if store.finalized_header.body_root.as_slice() != block_hash.as_ref() {
        debug!("Wrong beacon body");
        return;
    }

    let mut transaction_hashes = List::<Node, 1_048_576>::deserialize(&transaction_hashes[..]).expect("Unable to deserialize transaction hashes");
    let hash = transaction_hashes.hash_tree_root().expect("Unable to calculate transactions root");
    if hash != beacon_block_body_light.execution_payload_header().transactions_root() {
        debug!("Wrong transaction hashes");
        return;
    }

    blocks.push_back((beacon_block_body_light.execution_payload_header().clone(), transaction_hashes));
}
