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

#![cfg_attr(not(feature = "std"), no_std)]

use frame_support::{pallet_prelude::*, traits::EnsureOrigin, unsigned::ValidateUnsigned};
use frame_system::{
    offchain::{SendTransactionTypes, SubmitTransaction},
    pallet_prelude::*,
};
use log::{debug, trace};
use sp_core::ed25519;
use sp_runtime::{
    RuntimeDebug, Saturating,
    traits::{SaturatedConversion, Verify},
    transaction_validity::{
        InvalidTransaction, TransactionPriority, TransactionSource, TransactionValidity,
        ValidTransaction,
    },
};
use sp_std::{marker::PhantomData as _PhantomData, prelude::*};

pub use pallet::*;

pub type RequestId = u32;
pub type SetId = u64;

/// Provides GRANDPA authority data for a given set.
pub trait AuthorityProvider<AuthorityId> {
    /// Returns the current GRANDPA set id.
    fn current_set_id() -> SetId;
    /// Returns the authorities for a given set id. If the set id is unknown, return an empty list.
    fn authorities(set_id: SetId) -> Vec<AuthorityId>;
}

/// Reuse the GRANDPA key type for offchain signing.
pub const KEY_TYPE: sp_core::crypto::KeyTypeId = sp_consensus_grandpa::KEY_TYPE;

#[allow(clippy::manual_inspect)]
#[frame_support::pallet]
pub mod pallet {
    use super::*;

    #[derive(Clone, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen, RuntimeDebug)]
    #[scale_info(skip_type_params(T))]
    pub struct SigningRequest<T: Config> {
        pub id: RequestId,
        pub payload: BoundedVec<u8, T::MaxPayloadLength>,
        pub set_id: SetId,
        pub authorities: BoundedVec<T::AuthorityId, T::MaxAuthorities>,
        pub threshold: u32,
        pub created_at: BlockNumberFor<T>,
        pub expires_at: Option<BlockNumberFor<T>>,
    }

    #[pallet::pallet]
    #[pallet::without_storage_info]
    pub struct Pallet<T>(_PhantomData<T>);

    #[pallet::config]
    pub trait Config: frame_system::Config {
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        /// GRANDPA public key type.
        type AuthorityId: Parameter
            + Member
            + MaybeSerializeDeserialize
            + Ord
            + Clone
            + TypeInfo
            + MaxEncodedLen
            + AsRef<[u8]>;

        /// GRANDPA signature type.
        type AuthoritySignature: Parameter
            + Member
            + MaybeSerializeDeserialize
            + Clone
            + TypeInfo
            + MaxEncodedLen
            + Into<ed25519::Signature>;

        /// Origin allowed to schedule a signing request.
        type ScheduleOrigin: EnsureOrigin<Self::RuntimeOrigin>;

        /// Upper bounds.
        type MaxAuthorities: Get<u32>;
        type MaxPayloadLength: Get<u32>;
        type MaxRequests: Get<u32>;
        type MaxSignaturesPerRequest: Get<u32>;

        /// Priority used for unsigned submissions.
        type UnsignedPriority: Get<TransactionPriority>;

        /// Provider to read GRANDPA authorities and current set id.
        type AuthorityProvider: AuthorityProvider<Self::AuthorityId>;

        /// Weight information.
        type WeightInfo: WeightInfo;
    }

    #[pallet::storage]
    #[pallet::getter(fn next_request_id)]
    pub type NextRequestId<T> = StorageValue<_, RequestId, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn requests)]
    pub type Requests<T: Config> = StorageMap<_, Twox64Concat, RequestId, SigningRequest<T>>;

    #[pallet::storage]
    #[pallet::getter(fn signatures)]
    pub type Signatures<T: Config> = StorageDoubleMap<
        _,
        Twox64Concat,
        RequestId,
        Blake2_128Concat,
        T::AuthorityId,
        T::AuthoritySignature,
    >;

    #[pallet::storage]
    #[pallet::getter(fn signature_count)]
    pub type SignatureCount<T: Config> = StorageMap<_, Twox64Concat, RequestId, u32, ValueQuery>;

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        RequestScheduled {
            request_id: RequestId,
            set_id: SetId,
        },
        SignatureAdded {
            request_id: RequestId,
            authority: T::AuthorityId,
            count: u32,
        },
        ThresholdReached {
            request_id: RequestId,
        },
    }

    #[pallet::error]
    pub enum Error<T> {
        TooManyRequests,
        TooManyAuthorities,
        BadThreshold,
        UnknownRequest,
        RequestExpired,
        AuthorityNotInRequest,
        AlreadySigned,
        BadSignature,
        UnsupportedSet,
        PayloadTooLong,
        MaxSignaturesReached,
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// Schedule a signing request for the current GRANDPA set or provided subset.
        #[pallet::call_index(0)]
        #[pallet::weight(T::WeightInfo::schedule_request())]
        pub fn schedule_request(
            origin: OriginFor<T>,
            payload: Vec<u8>,
            set_id: Option<SetId>,
            authorities: Option<Vec<T::AuthorityId>>,
            threshold: u32,
            expires_at: Option<BlockNumberFor<T>>,
        ) -> DispatchResult {
            T::ScheduleOrigin::ensure_origin(origin)?;

            let req_id = NextRequestId::<T>::get();
            ensure!(req_id < T::MaxRequests::get(), Error::<T>::TooManyRequests);

            let set_id = set_id.unwrap_or_else(T::AuthorityProvider::current_set_id);
            ensure!(
                set_id == T::AuthorityProvider::current_set_id(),
                Error::<T>::UnsupportedSet
            );

            let authorities = match authorities {
                Some(list) => list,
                None => T::AuthorityProvider::authorities(set_id),
            };

            ensure!(
                !authorities.is_empty() && authorities.len() as u32 <= T::MaxAuthorities::get(),
                Error::<T>::TooManyAuthorities
            );
            ensure!(
                threshold > 0 && threshold as usize <= authorities.len(),
                Error::<T>::BadThreshold
            );
            ensure!(
                threshold <= T::MaxSignaturesPerRequest::get(),
                Error::<T>::BadThreshold
            );

            let bounded_payload: BoundedVec<_, T::MaxPayloadLength> =
                payload.try_into().map_err(|_| Error::<T>::PayloadTooLong)?;

            let bounded_authorities: BoundedVec<_, T::MaxAuthorities> = authorities
                .try_into()
                .map_err(|_| Error::<T>::TooManyAuthorities)?;

            let now = <frame_system::Pallet<T>>::block_number();

            let request = SigningRequest {
                id: req_id,
                payload: bounded_payload,
                set_id,
                authorities: bounded_authorities,
                threshold,
                created_at: now,
                expires_at,
            };

            Requests::<T>::insert(req_id, request);
            NextRequestId::<T>::put(req_id + 1);

            Self::deposit_event(Event::RequestScheduled {
                request_id: req_id,
                set_id,
            });
            Ok(())
        }

        /// Submit a GRANDPA signature for a scheduled request (unsigned).
        #[pallet::call_index(1)]
        #[pallet::weight(T::WeightInfo::submit_signature())]
        pub fn submit_signature(
            origin: OriginFor<T>,
            request_id: RequestId,
            authority: T::AuthorityId,
            signature: T::AuthoritySignature,
        ) -> DispatchResult {
            ensure_none(origin)?;
            Self::process_signature(request_id, authority, signature)
        }
    }

    impl<T: Config> Pallet<T> {
        fn process_signature(
            request_id: RequestId,
            authority: T::AuthorityId,
            signature: T::AuthoritySignature,
        ) -> DispatchResult {
            let request = Requests::<T>::get(request_id).ok_or(Error::<T>::UnknownRequest)?;
            Self::validate_request(&request)?;

            ensure!(
                request.authorities.contains(&authority),
                Error::<T>::AuthorityNotInRequest
            );
            ensure!(
                !Signatures::<T>::contains_key(request_id, &authority),
                Error::<T>::AlreadySigned
            );
            ensure!(
                SignatureCount::<T>::get(request_id) < T::MaxSignaturesPerRequest::get(),
                Error::<T>::MaxSignaturesReached
            );

            let message = request.payload.clone();
            ensure!(
                Self::verify_sig(&authority, &signature, &message),
                Error::<T>::BadSignature
            );

            Signatures::<T>::insert(request_id, &authority, signature);
            let count = SignatureCount::<T>::mutate(request_id, |c| {
                *c = c.saturating_add(1);
                *c
            });

            Self::deposit_event(Event::SignatureAdded {
                request_id,
                authority: authority.clone(),
                count,
            });

            if count >= request.threshold {
                Self::deposit_event(Event::ThresholdReached { request_id });
            }

            Ok(())
        }

        fn validate_request(request: &SigningRequest<T>) -> Result<(), Error<T>> {
            ensure!(
                request.set_id == T::AuthorityProvider::current_set_id(),
                Error::<T>::UnsupportedSet
            );
            if let Some(exp) = request.expires_at {
                let now = <frame_system::Pallet<T>>::block_number();
                ensure!(now <= exp, Error::<T>::RequestExpired);
            }
            Ok(())
        }

        fn verify_sig(
            authority: &T::AuthorityId,
            signature: &T::AuthoritySignature,
            payload: &[u8],
        ) -> bool {
            let Ok(raw): Result<[u8; 32], _> = authority.as_ref().try_into() else {
                return false;
            };
            let pubkey = ed25519::Public::from_raw(raw);
            let sig: ed25519::Signature = signature.clone().into();
            sig.verify(payload, &pubkey)
        }
    }

    #[pallet::validate_unsigned]
    impl<T: Config> ValidateUnsigned for Pallet<T> {
        type Call = Call<T>;

        fn validate_unsigned(source: TransactionSource, call: &Self::Call) -> TransactionValidity {
            if !matches!(
                source,
                TransactionSource::External | TransactionSource::Local | TransactionSource::InBlock
            ) {
                return InvalidTransaction::Call.into();
            }

            match call {
                Call::submit_signature {
                    request_id,
                    authority,
                    signature,
                } => {
                    let request = if let Some(r) = Requests::<T>::get(request_id) {
                        r
                    } else {
                        return InvalidTransaction::Call.into();
                    };
                    if let Err(err) = Self::validate_request(&request) {
                        return match err {
                            Error::<T>::RequestExpired => InvalidTransaction::Stale.into(),
                            Error::<T>::UnsupportedSet => InvalidTransaction::BadProof.into(),
                            _ => InvalidTransaction::Call.into(),
                        };
                    }
                    if !request.authorities.contains(authority) {
                        return InvalidTransaction::BadProof.into();
                    }
                    if Signatures::<T>::contains_key(request_id, authority) {
                        return InvalidTransaction::Stale.into();
                    }
                    if SignatureCount::<T>::get(request_id) >= T::MaxSignaturesPerRequest::get() {
                        return InvalidTransaction::Stale.into();
                    }
                    if !Self::verify_sig(authority, signature, &request.payload) {
                        return InvalidTransaction::BadProof.into();
                    }

                    let provides = vec![(b"grandpa-signer", request_id, authority).encode()];
                    let longevity = request
                        .expires_at
                        .and_then(|exp| {
                            let now = <frame_system::Pallet<T>>::block_number();
                            exp.saturating_sub(now).try_into().ok()
                        })
                        .unwrap_or(TransactionLongevity::MAX);

                    Ok(ValidTransaction {
                        priority: T::UnsignedPriority::get(),
                        requires: vec![],
                        provides,
                        longevity,
                        propagate: true,
                    })
                }
                _ => InvalidTransaction::Call.into(),
            }
        }
    }

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T>
    where
        T: SendTransactionTypes<Call<T>>,
        T::AuthorityId: From<ed25519::Public>,
        T::AuthoritySignature: From<ed25519::Signature>,
    {
        fn offchain_worker(_n: BlockNumberFor<T>) {
            const MAX_REQUESTS_PER_WORKER: usize = 64;
            const MAX_SUBMISSIONS_PER_WORKER: usize = 128;
            const BACKOFF_BLOCKS: u64 = 5;

            let local_keys = sp_io::crypto::ed25519_public_keys(KEY_TYPE);
            if local_keys.is_empty() {
                return;
            }

            let mut submissions = 0usize;

            for (request_id, request) in Requests::<T>::iter()
                .take(MAX_REQUESTS_PER_WORKER.min(T::MaxRequests::get() as usize))
            {
                if SignatureCount::<T>::get(request_id) >= request.threshold {
                    trace!(target: "grandpa-signer", "skip request {}: threshold met", request_id);
                    continue;
                }
                if let Err(err) = Self::validate_request(&request) {
                    trace!(target: "grandpa-signer", "skip request {}: {:?}", request_id, err);
                    continue;
                }
                if SignatureCount::<T>::get(request_id) >= T::MaxSignaturesPerRequest::get() {
                    trace!(target: "grandpa-signer", "skip request {}: max signatures reached", request_id);
                    continue;
                }

                let now = <frame_system::Pallet<T>>::block_number().saturated_into::<u64>();
                let mut key = b"grandpa-signer:last_attempt:".to_vec();
                key.extend_from_slice(&request_id.to_le_bytes());
                if let Some(bytes) = sp_io::offchain::local_storage_get(
                    sp_core::offchain::StorageKind::PERSISTENT,
                    &key,
                ) && bytes.len() == 8
                {
                    let last = u64::from_le_bytes(bytes.as_slice().try_into().unwrap());
                    if now.saturating_sub(last) < BACKOFF_BLOCKS {
                        trace!(target: "grandpa-signer", "skip request {}: backoff", request_id);
                        continue;
                    }
                }

                let mut submitted = false;

                for key in local_keys.iter() {
                    if submissions >= MAX_SUBMISSIONS_PER_WORKER {
                        return;
                    }
                    let authority: T::AuthorityId = (*key).into();

                    if !request.authorities.contains(&authority) {
                        trace!(target: "grandpa-signer", "skip key for request {}: not in authorities", request_id);
                        continue;
                    }
                    if Signatures::<T>::contains_key(request_id, &authority) {
                        trace!(target: "grandpa-signer", "skip key for request {}: already signed", request_id);
                        continue;
                    }
                    if SignatureCount::<T>::get(request_id) >= T::MaxSignaturesPerRequest::get() {
                        break;
                    }

                    if let Some(signature) =
                        sp_io::crypto::ed25519_sign(KEY_TYPE, key, &request.payload)
                    {
                        let signature: T::AuthoritySignature = signature.into();
                        let call = Call::submit_signature {
                            request_id,
                            authority,
                            signature,
                        };
                        match SubmitTransaction::<T, Call<T>>::submit_unsigned_transaction(
                            call.into(),
                        ) {
                            Ok(()) => {
                                submissions = submissions.saturating_add(1);
                                submitted = true;
                                sp_io::offchain::local_storage_set(
                                    sp_core::offchain::StorageKind::PERSISTENT,
                                    key,
                                    &now.to_le_bytes(),
                                );
                                debug!(target: "grandpa-signer", "submitted signature for request {}", request_id);
                            }
                            Err(e) => {
                                sp_io::offchain::local_storage_set(
                                    sp_core::offchain::StorageKind::PERSISTENT,
                                    key,
                                    &now.to_le_bytes(),
                                );
                                debug!(target: "grandpa-signer", "failed to submit signature for request {}: {:?}", request_id, e);
                            }
                        }
                    }
                    if submitted {
                        break;
                    }
                }
            }
        }
    }

    /// Weight functions for this pallet.
    pub trait WeightInfo {
        fn schedule_request() -> Weight;
        fn submit_signature() -> Weight;
    }

    impl WeightInfo for () {
        fn schedule_request() -> Weight {
            Weight::from_parts(10_000, 0)
        }
        fn submit_signature() -> Weight {
            Weight::from_parts(10_000, 0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate as pallet_grandpa_signer;
    use frame_support::{assert_noop, assert_ok, parameter_types};
    use sp_core::{Pair, ed25519};
    use sp_runtime::{BuildStorage, traits::IdentityLookup};

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
        pub const MaxAuthorities: u32 = 4;
        pub const MaxPayloadLength: u32 = 64;
        pub const MaxRequests: u32 = 16;
        pub const MaxSignaturesPerRequest: u32 = 4;
        pub const UnsignedPriority: TransactionPriority = 1_000_000;
    }

    pub struct TestAuthorityProvider;
    impl AuthorityProvider<ed25519::Public> for TestAuthorityProvider {
        fn current_set_id() -> SetId {
            1
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
        type ScheduleOrigin = frame_system::EnsureRoot<u64>;
        type MaxAuthorities = MaxAuthorities;
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
        let t = frame_system::GenesisConfig::<Test>::default()
            .build_storage()
            .unwrap();
        sp_io::TestExternalities::new(t)
    }

    #[test]
    fn schedule_and_submit_signature_works() {
        new_ext().execute_with(|| {
            System::set_block_number(1);
            let payload = b"hello".to_vec();
            assert_ok!(GrandpaSigner::schedule_request(
                RuntimeOrigin::root(),
                payload.clone(),
                None,
                None,
                1,
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
    }

    #[test]
    fn duplicate_signature_rejected() {
        new_ext().execute_with(|| {
            System::set_block_number(1);
            let payload = b"hello".to_vec();
            assert_ok!(GrandpaSigner::schedule_request(
                RuntimeOrigin::root(),
                payload.clone(),
                None,
                None,
                1,
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
    }

    #[test]
    fn expired_request_rejected() {
        new_ext().execute_with(|| {
            System::set_block_number(1);
            let payload = b"hello".to_vec();
            assert_ok!(GrandpaSigner::schedule_request(
                RuntimeOrigin::root(),
                payload.clone(),
                None,
                None,
                1,
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
    }

    #[test]
    fn bad_signature_rejected() {
        new_ext().execute_with(|| {
            System::set_block_number(1);
            let payload = b"hello".to_vec();
            assert_ok!(GrandpaSigner::schedule_request(
                RuntimeOrigin::root(),
                payload.clone(),
                None,
                None,
                1,
                None
            ));
            let pair = &auth_keys()[0];
            let sig = pair.sign(b"other");
            assert_noop!(
                GrandpaSigner::submit_signature(RuntimeOrigin::none(), 0, pair.public(), sig),
                Error::<Test>::BadSignature
            );
        });
    }
}
