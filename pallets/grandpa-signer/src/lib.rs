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

use frame_support::{
    pallet_prelude::*, unsigned::ValidateUnsigned, weights::constants::RocksDbWeight,
};
use frame_system::{
    ensure_root,
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

        /// Upper bounds.
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
    }

    #[pallet::error]
    pub enum Error<T> {
        TooManyRequests,
        UnknownRequest,
        RequestExpired,
        AuthorityNotInSet,
        AlreadySigned,
        BadSignature,
        UnsupportedSet,
        PayloadTooLong,
        MaxSignaturesReached,
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// Schedule a signing request for the current GRANDPA set.
        #[pallet::call_index(0)]
        #[pallet::weight(T::WeightInfo::schedule_request())]
        pub fn schedule_request(
            origin: OriginFor<T>,
            payload: Vec<u8>,
            set_id: Option<SetId>,
            expires_at: Option<BlockNumberFor<T>>,
        ) -> DispatchResult {
            ensure_root(origin)?;

            let now = <frame_system::Pallet<T>>::block_number();
            Self::prune_expired_requests(now);

            let active_requests = Requests::<T>::iter().count();
            ensure!(
                active_requests < T::MaxRequests::get() as usize,
                Error::<T>::TooManyRequests
            );

            let req_id = NextRequestId::<T>::get();

            let set_id = set_id.unwrap_or_else(T::AuthorityProvider::current_set_id);
            ensure!(
                set_id == T::AuthorityProvider::current_set_id(),
                Error::<T>::UnsupportedSet
            );

            let bounded_payload: BoundedVec<_, T::MaxPayloadLength> =
                payload.try_into().map_err(|_| Error::<T>::PayloadTooLong)?;

            let request = SigningRequest {
                id: req_id,
                payload: bounded_payload,
                set_id,
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

            let set_id = T::AuthorityProvider::current_set_id();
            ensure!(request.set_id == set_id, Error::<T>::UnsupportedSet);
            let authorities = T::AuthorityProvider::authorities(set_id);
            ensure!(
                authorities.contains(&authority),
                Error::<T>::AuthorityNotInSet
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

            let authorities = T::AuthorityProvider::authorities(request.set_id);
            if count >= authorities.len() as u32 || count >= T::MaxSignaturesPerRequest::get() {
                Self::cleanup_request(request_id);
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

        fn cleanup_request(request_id: RequestId) {
            Requests::<T>::remove(request_id);
            SignatureCount::<T>::remove(request_id);
            let _ = Signatures::<T>::clear_prefix(request_id, u32::MAX, None);
        }

        fn prune_expired_requests(now: BlockNumberFor<T>) {
            for (request_id, request) in Requests::<T>::iter() {
                if let Some(exp) = request.expires_at
                    && now > exp
                {
                    Self::cleanup_request(request_id);
                }
            }
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
                    let set_id = T::AuthorityProvider::current_set_id();
                    let authorities = T::AuthorityProvider::authorities(set_id);
                    if !authorities.contains(authority) {
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
                    let longevity = match request.expires_at {
                        Some(exp) => {
                            let now = <frame_system::Pallet<T>>::block_number();
                            let remaining = exp.saturating_sub(now);
                            match remaining.try_into() {
                                Ok(l) if l > 0 => l,
                                _ => return InvalidTransaction::Stale.into(),
                            }
                        }
                        None => TransactionLongevity::MAX,
                    };

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
                if let Err(err) = Self::validate_request(&request) {
                    trace!(target: "grandpa-signer", "skip request {}: {:?}", request_id, err);
                    continue;
                }
                if SignatureCount::<T>::get(request_id) >= T::MaxSignaturesPerRequest::get() {
                    trace!(target: "grandpa-signer", "skip request {}: max signatures reached", request_id);
                    continue;
                }
                let authorities = T::AuthorityProvider::authorities(request.set_id);
                if authorities.is_empty() {
                    trace!(target: "grandpa-signer", "skip request {}: no authorities", request_id);
                    continue;
                }

                let now = <frame_system::Pallet<T>>::block_number().saturated_into::<u64>();
                let mut last_attempt_key = b"grandpa-signer:last_attempt:".to_vec();
                last_attempt_key.extend_from_slice(&request_id.to_le_bytes());
                if let Some(bytes) = sp_io::offchain::local_storage_get(
                    sp_core::offchain::StorageKind::PERSISTENT,
                    &last_attempt_key,
                ) && bytes.len() == 8
                {
                    let last = u64::from_le_bytes(bytes.as_slice().try_into().unwrap());
                    if now.saturating_sub(last) < BACKOFF_BLOCKS {
                        trace!(target: "grandpa-signer", "skip request {}: backoff", request_id);
                        continue;
                    }
                }

                let mut submitted = false;

                for local_key in local_keys.iter() {
                    if submissions >= MAX_SUBMISSIONS_PER_WORKER {
                        return;
                    }
                    let authority: T::AuthorityId = (*local_key).into();

                    if !authorities.contains(&authority) {
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
                        sp_io::crypto::ed25519_sign(KEY_TYPE, local_key, &request.payload)
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
                                    &last_attempt_key,
                                    &now.to_le_bytes(),
                                );
                                debug!(target: "grandpa-signer", "submitted signature for request {}", request_id);
                            }
                            Err(e) => {
                                sp_io::offchain::local_storage_set(
                                    sp_core::offchain::StorageKind::PERSISTENT,
                                    &last_attempt_key,
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

    /// Fallback weights used when no benchmarking data is supplied.
    pub struct SubstrateWeight<T>(sp_std::marker::PhantomData<T>);

    impl<T: Config> WeightInfo for SubstrateWeight<T> {
        fn schedule_request() -> Weight {
            // Reads: NextRequestId; Requests (prune pass).
            // Writes: Requests, NextRequestId; cleanup for up to MaxRequests expired entries and their signatures.
            let db = T::DbWeight::get();
            let max_requests = T::MaxRequests::get() as u64;
            let max_sigs = T::MaxSignaturesPerRequest::get() as u64;
            Weight::from_parts(55_000_000, 2048)
                .saturating_add(db.reads(1 + max_requests))
                .saturating_add(db.writes(2 + max_requests * (2 + max_sigs)))
        }

        fn submit_signature() -> Weight {
            // Reads: Requests, Signatures, SignatureCount.
            // Writes: Signatures, SignatureCount; optional cleanup of a completed request.
            let db = T::DbWeight::get();
            let max_sigs = T::MaxSignaturesPerRequest::get() as u64;
            Weight::from_parts(145_000_000, 4096)
                .saturating_add(db.reads(3))
                .saturating_add(db.writes(2 + 1 + max_sigs))
        }
    }

    // Backward-compatible fallback for tests/benches that still select `()`.
    impl WeightInfo for () {
        fn schedule_request() -> Weight {
            // Rough upper bound; prefer `SubstrateWeight` in runtimes.
            Weight::from_parts(55_000_000, 2048)
                .saturating_add(RocksDbWeight::get().reads(2_u64))
                .saturating_add(RocksDbWeight::get().writes(3_u64))
        }

        fn submit_signature() -> Weight {
            Weight::from_parts(145_000_000, 4096)
                .saturating_add(RocksDbWeight::get().reads(3_u64))
                .saturating_add(RocksDbWeight::get().writes(5_u64))
        }
    }
}

#[cfg(test)]
mod tests;
