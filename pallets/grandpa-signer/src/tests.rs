// This file is part of Gear.
//
// Copyright (C) 2021-2025 Gear Technologies Inc.
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
use crate as pallet_grandpa_signer;
use core::sync::atomic::{AtomicU64, Ordering};
use frame_support::{assert_noop, assert_ok, parameter_types};
use sp_core::{Pair, ed25519};
use sp_runtime::{BuildStorage, traits::IdentityLookup};
use std::sync::Mutex;

type Extrinsic = sp_runtime::testing::TestXt<Call<Test>, ()>;
type Block = frame_system::mocking::MockBlock<Test>;

frame_support::construct_runtime!(
    pub enum Test {
        System: frame_system,
        GrandpaSigner: pallet_grandpa_signer,
    }
);

parameter_types! {
    pub const BlockHashCount: u64 = 250;
    pub const MaxPayloadLength: u32 = 64;
    pub const MaxRequests: u32 = 16;
    pub const MaxSignaturesPerRequest: u32 = 4;
    pub const UnsignedPriority: TransactionPriority = 1_000_000;
}

pub struct TestAuthorityProvider;
static SET_ID: AtomicU64 = AtomicU64::new(1);
static SET_ID_LOCK: Mutex<()> = Mutex::new(());

impl AuthorityProvider<ed25519::Public> for TestAuthorityProvider {
    fn current_set_id() -> SetId {
        SET_ID.load(Ordering::SeqCst)
    }

    fn authorities(set_id: SetId) -> Vec<ed25519::Public> {
        if set_id == 1 {
            auth_keys().into_iter().map(|p| p.public()).collect()
        } else {
            Vec::new()
        }
    }
}

impl frame_system::Config for Test {
    type BaseCallFilter = frame_support::traits::Everything;
    type BlockWeights = ();
    type BlockLength = ();
    type DbWeight = ();
    type RuntimeOrigin = RuntimeOrigin;
    type RuntimeCall = RuntimeCall;
    type RuntimeEvent = RuntimeEvent;
    type Block = Block;
    type Hash = sp_core::H256;
    type Hashing = sp_runtime::traits::BlakeTwo256;
    type AccountId = u64;
    type Lookup = IdentityLookup<Self::AccountId>;
    type Nonce = u64;
    type BlockHashCount = BlockHashCount;
    type Version = ();
    type PalletInfo = PalletInfo;
    type AccountData = ();
    type OnNewAccount = ();
    type OnKilledAccount = ();
    type SystemWeightInfo = ();
    type SS58Prefix = ();
    type OnSetCode = ();
    type MaxConsumers = frame_support::traits::ConstU32<16>;
    type RuntimeTask = ();
    type SingleBlockMigrations = ();
    type MultiBlockMigrator = ();
    type PreInherents = ();
    type PostInherents = ();
    type PostTransactions = ();
}

impl frame_system::offchain::SendTransactionTypes<Call<Test>> for Test {
    type Extrinsic = Extrinsic;
    type OverarchingCall = Call<Test>;
}

impl Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type AuthorityId = ed25519::Public;
    type AuthoritySignature = ed25519::Signature;
    type MaxPayloadLength = MaxPayloadLength;
    type MaxRequests = MaxRequests;
    type MaxSignaturesPerRequest = MaxSignaturesPerRequest;
    type UnsignedPriority = UnsignedPriority;
    type AuthorityProvider = TestAuthorityProvider;
    type WeightInfo = ();
}

fn auth_keys() -> Vec<ed25519::Pair> {
    vec![
        ed25519::Pair::from_seed_slice(&[1u8; 32]).expect("seed"),
        ed25519::Pair::from_seed_slice(&[2u8; 32]).expect("seed"),
        ed25519::Pair::from_seed_slice(&[3u8; 32]).expect("seed"),
    ]
}

fn new_ext() -> sp_io::TestExternalities {
    SET_ID.store(1, Ordering::SeqCst);
    let t = frame_system::GenesisConfig::<Test>::default()
        .build_storage()
        .unwrap();
    sp_io::TestExternalities::new(t)
}

fn set_set_id(id: SetId) {
    SET_ID.store(id, Ordering::SeqCst);
}

fn with_set_id_lock<R>(f: impl FnOnce() -> R) -> R {
    let _guard = SET_ID_LOCK.lock().expect("lock poisoned");
    f()
}

#[test]
fn schedule_and_submit_signature_works() {
    with_set_id_lock(|| {
        new_ext().execute_with(|| {
            System::set_block_number(1);
            let payload = b"hello".to_vec();
            assert_ok!(GrandpaSigner::schedule_request(
                RuntimeOrigin::root(),
                payload.clone(),
                None,
                None
            ));

            let req = GrandpaSigner::requests(0).expect("request created");
            let pair = &auth_keys()[0];
            let sig = pair.sign(&payload);
            assert_ok!(GrandpaSigner::submit_signature(
                RuntimeOrigin::none(),
                req.id,
                pair.public(),
                sig
            ));

            assert_eq!(GrandpaSigner::signature_count(req.id), 1);
        });
    })
}

#[test]
fn duplicate_signature_rejected() {
    with_set_id_lock(|| {
        new_ext().execute_with(|| {
            System::set_block_number(1);
            let payload = b"hello".to_vec();
            assert_ok!(GrandpaSigner::schedule_request(
                RuntimeOrigin::root(),
                payload.clone(),
                None,
                None
            ));
            let req = GrandpaSigner::requests(0).unwrap();
            let pair = &auth_keys()[0];
            let sig = pair.sign(&payload);
            assert_ok!(GrandpaSigner::submit_signature(
                RuntimeOrigin::none(),
                req.id,
                pair.public(),
                sig
            ));
            assert_noop!(
                GrandpaSigner::submit_signature(RuntimeOrigin::none(), req.id, pair.public(), sig),
                Error::<Test>::AlreadySigned
            );
        });
    })
}

#[test]
fn expired_request_rejected() {
    with_set_id_lock(|| {
        new_ext().execute_with(|| {
            System::set_block_number(1);
            let payload = b"hello".to_vec();
            assert_ok!(GrandpaSigner::schedule_request(
                RuntimeOrigin::root(),
                payload.clone(),
                None,
                Some(2)
            ));
            System::set_block_number(3);
            let pair = &auth_keys()[0];
            let sig = pair.sign(&payload);
            assert_noop!(
                GrandpaSigner::submit_signature(RuntimeOrigin::none(), 0, pair.public(), sig),
                Error::<Test>::RequestExpired
            );
        });
    })
}

#[test]
fn bad_signature_rejected() {
    with_set_id_lock(|| {
        new_ext().execute_with(|| {
            System::set_block_number(1);
            let payload = b"hello".to_vec();
            assert_ok!(GrandpaSigner::schedule_request(
                RuntimeOrigin::root(),
                payload.clone(),
                None,
                None
            ));
            let pair = &auth_keys()[0];
            let sig = pair.sign(b"other");
            assert_noop!(
                GrandpaSigner::submit_signature(RuntimeOrigin::none(), 0, pair.public(), sig),
                Error::<Test>::BadSignature
            );
        });
    })
}

#[test]
fn stale_set_requests_are_pruned_on_schedule() {
    with_set_id_lock(|| {
        new_ext().execute_with(|| {
            System::set_block_number(1);
            let payload = b"hello".to_vec();
            assert_ok!(GrandpaSigner::schedule_request(
                RuntimeOrigin::root(),
                payload.clone(),
                None,
                None
            ));
            assert!(GrandpaSigner::requests(0).is_some());

            // Simulate authority set rotation.
            set_set_id(2);

            // The stale request from set 1 should be pruned before counting against capacity.
            assert_ok!(GrandpaSigner::schedule_request(
                RuntimeOrigin::root(),
                payload,
                None,
                None
            ));
            assert!(GrandpaSigner::requests(0).is_none());
            assert_eq!(GrandpaSigner::requests(1).unwrap().set_id, 2);
        });
    })
}

#[test]
fn identical_payload_can_be_signed_for_multiple_requests() {
    with_set_id_lock(|| {
        new_ext().execute_with(|| {
            System::set_block_number(1);
            let payload = b"raw-payload".to_vec();

            assert_ok!(GrandpaSigner::schedule_request(
                RuntimeOrigin::root(),
                payload.clone(),
                None,
                None
            ));
            assert_ok!(GrandpaSigner::schedule_request(
                RuntimeOrigin::root(),
                payload.clone(),
                None,
                None
            ));

            let pair = &auth_keys()[0];
            let sig = pair.sign(&payload);

            assert_ok!(GrandpaSigner::submit_signature(
                RuntimeOrigin::none(),
                0,
                pair.public(),
                sig.clone()
            ));
            assert_ok!(GrandpaSigner::submit_signature(
                RuntimeOrigin::none(),
                1,
                pair.public(),
                sig
            ));

            assert_eq!(GrandpaSigner::signature_count(0), 1);
            assert_eq!(GrandpaSigner::signature_count(1), 1);
        });
    })
}
