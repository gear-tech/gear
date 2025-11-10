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

use crate::{
    Address, Announce, HashOf,
    consensus::{BatchCommitmentValidationReply, BatchCommitmentValidationRequest},
    crypto::ToDigest,
    ecdsa::{SignedData, VerifiedData},
    injected::InjectedPromise,
};
use alloc::vec::Vec;
use core::{hash::Hash, num::NonZeroU32};
use gprimitives::H256;
use parity_scale_codec::{Decode, Encode};
use sha3::{Digest as _, Keccak256};

pub type ValidatorAnnounce = ValidatorMessage<Announce>;
pub type ValidatorRequest = ValidatorMessage<BatchCommitmentValidationRequest>;
pub type ValidatorReply = ValidatorMessage<BatchCommitmentValidationReply>;
pub type ValidatorInjectedPromise = ValidatorMessage<InjectedPromise>;

#[derive(Debug, Clone, Encode, Decode, Eq, PartialEq, Hash)]
pub struct ValidatorMessage<T> {
    pub block: H256,
    pub payload: T,
}

impl<T: ToDigest> ToDigest for ValidatorMessage<T> {
    fn update_hasher(&self, hasher: &mut Keccak256) {
        let Self { block, payload } = self;
        hasher.update(block.0);
        payload.update_hasher(hasher);
    }
}

#[derive(Debug, Clone, Encode, Decode, Eq, PartialEq, derive_more::Unwrap, derive_more::From)]
pub enum SignedValidatorMessage {
    ProducerBlock(SignedData<ValidatorAnnounce>),
    RequestBatchValidation(SignedData<ValidatorRequest>),
    ApproveBatch(SignedData<ValidatorReply>),
    InjectedPromise(SignedData<ValidatorInjectedPromise>),
}

impl SignedValidatorMessage {
    pub fn into_verified(self) -> VerifiedValidatorMessage {
        match self {
            SignedValidatorMessage::ProducerBlock(announce) => announce.into_verified().into(),
            SignedValidatorMessage::RequestBatchValidation(request) => {
                request.into_verified().into()
            }
            SignedValidatorMessage::ApproveBatch(reply) => reply.into_verified().into(),
            SignedValidatorMessage::InjectedPromise(promise) => promise.into_verified().into(),
        }
    }
}

#[cfg_attr(feature = "serde", derive(Hash))]
#[derive(Debug, Clone, Eq, PartialEq, derive_more::Unwrap, derive_more::From)]
pub enum VerifiedValidatorMessage {
    ProducerBlock(VerifiedData<ValidatorAnnounce>),
    RequestBatchValidation(VerifiedData<ValidatorRequest>),
    ApproveBatch(VerifiedData<ValidatorReply>),
    InjectedPromise(VerifiedData<ValidatorInjectedPromise>),
}

impl VerifiedValidatorMessage {
    pub fn block(&self) -> H256 {
        match self {
            VerifiedValidatorMessage::ProducerBlock(announce) => announce.data().block,
            VerifiedValidatorMessage::RequestBatchValidation(request) => request.data().block,
            VerifiedValidatorMessage::ApproveBatch(reply) => reply.data().block,
            VerifiedValidatorMessage::InjectedPromise(promise) => promise.data().block,
        }
    }

    pub fn address(&self) -> Address {
        match self {
            VerifiedValidatorMessage::ProducerBlock(announce) => announce.address(),
            VerifiedValidatorMessage::RequestBatchValidation(request) => request.address(),
            VerifiedValidatorMessage::ApproveBatch(reply) => reply.address(),
            VerifiedValidatorMessage::InjectedPromise(promise) => promise.address(),
        }
    }
}

/// Until condition for announces request (see [`AnnouncesRequest`]).
#[derive(PartialEq, Eq, Hash, Debug, Clone, Copy, Encode, Decode)]
pub enum AnnouncesRequestUntil {
    /// Request until a specific tail announce hash
    Tail(HashOf<Announce>),
    /// Request until a specific chain length
    ChainLen(NonZeroU32),
}

/// Request announces body (see [`Announce`]) chain from `head_announce_hash` to `tail_announce_hash`.
/// `tail_announce_hash` must not be included in response.
/// If `tail_announce_hash` is None, then only data `head_announce_hash` must be returned.
#[derive(PartialEq, Eq, Hash, Debug, Clone, Copy, Encode, Decode)]
pub struct AnnouncesRequest {
    /// Hash of the requested chain head announce
    pub head: HashOf<Announce>,
    /// Request until this condition is met
    pub until: AnnouncesRequestUntil,
}

// TODO #4911: can be optimized - only not-base announces could be returned.
/// Response for announces request.
/// Must contain all announces for the requested range.
/// Must be sorted from predecessors to successors.
#[cfg_attr(feature = "serde", derive(Hash))]
#[derive(PartialEq, Eq, Debug, Clone, Default, Encode, Decode)]
pub struct AnnouncesResponse {
    /// List of announces
    pub announces: Vec<Announce>,
}

/// Checked announces response ensuring that it matches the corresponding request.
#[derive(derive_more::Debug, Clone, Eq, PartialEq, derive_more::From)]
pub struct CheckedAnnouncesResponse {
    /// Corresponding request for this response
    request: AnnouncesRequest,
    /// List of announces
    announces: Vec<Announce>,
}

#[derive(Debug, derive_more::Display)]
pub enum AnnouncesResponseError {
    #[display("response is empty")]
    Empty,
    #[display("announces head mismatch, expected hash {expected}, received {received}")]
    HeadMismatch {
        expected: HashOf<Announce>,
        received: HashOf<Announce>,
    },
    #[display("announces tail mismatch, expected hash {expected}, received {received}")]
    TailMismatch {
        expected: HashOf<Announce>,
        received: HashOf<Announce>,
    },
    #[display("announces len expected {expected}, received {received}")]
    LenMismatch { expected: usize, received: usize },
    #[display("announces chain is not linked")]
    ChainIsNotLinked,
}

impl AnnouncesResponse {
    pub fn try_into_checked(
        self,
        request: AnnouncesRequest,
    ) -> Result<CheckedAnnouncesResponse, AnnouncesResponseError> {
        let Self { announces } = self;

        let Some((tail, head)) = announces.first().zip(announces.last()) else {
            return Err(AnnouncesResponseError::Empty);
        };

        if request.head != head.to_hash() {
            return Err(AnnouncesResponseError::HeadMismatch {
                expected: request.head,
                received: head.to_hash(),
            });
        }

        let response_tail_hash = tail.to_hash();
        match request.until {
            AnnouncesRequestUntil::Tail(request_tail_hash) => {
                if request_tail_hash != response_tail_hash {
                    return Err(AnnouncesResponseError::TailMismatch {
                        expected: request_tail_hash,
                        received: response_tail_hash,
                    });
                }
            }
            AnnouncesRequestUntil::ChainLen(len) => {
                if announces.len() != len.get() as usize {
                    return Err(AnnouncesResponseError::LenMismatch {
                        expected: len.get() as usize,
                        received: announces.len(),
                    });
                }
            }
        }

        // Check chain linking
        let mut expected_parent_hash = response_tail_hash;
        for announce in announces.iter().skip(1) {
            if announce.parent != expected_parent_hash {
                return Err(AnnouncesResponseError::ChainIsNotLinked);
            }
            expected_parent_hash = announce.to_hash();
        }

        Ok(CheckedAnnouncesResponse { request, announces })
    }
}

impl CheckedAnnouncesResponse {
    pub fn request(&self) -> &AnnouncesRequest {
        &self.request
    }

    pub fn announces(&self) -> &[Announce] {
        &self.announces
    }

    pub fn into_parts(self) -> (AnnouncesRequest, Vec<Announce>) {
        (self.request, self.announces)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gprimitives::H256;

    fn make_chain(len: usize) -> Vec<Announce> {
        assert!(len > 0);
        let mut chain = Vec::with_capacity(len);
        let mut parent = HashOf::zero();

        for idx in 0..len {
            let announce = Announce::base(H256([idx as u8 + 1; 32]), parent);
            parent = announce.to_hash();
            chain.push(announce);
        }

        chain
    }

    #[test]
    fn try_into_checked_accepts_valid_tail_range() {
        let announces = make_chain(3);
        let head_hash = announces.last().unwrap().to_hash();
        let tail_hash = announces.first().unwrap().to_hash();

        let request = AnnouncesRequest {
            head: head_hash,
            until: AnnouncesRequestUntil::Tail(tail_hash),
        };
        let response = AnnouncesResponse {
            announces: announces.clone(),
        };

        let checked = response
            .try_into_checked(request)
            .expect("valid tail response");
        assert_eq!(checked.request(), &request);
        assert_eq!(checked.announces(), announces.as_slice());
    }

    #[test]
    fn try_into_checked_accepts_valid_chain_len() {
        let announces = make_chain(4);
        let head_hash = announces.last().unwrap().to_hash();

        let request = AnnouncesRequest {
            head: head_hash,
            until: AnnouncesRequestUntil::ChainLen((announces.len() as u32).try_into().unwrap()),
        };

        let response = AnnouncesResponse {
            announces: announces.clone(),
        };

        let checked = response
            .try_into_checked(request)
            .expect("valid len response");
        assert_eq!(checked.request(), &request);
        assert_eq!(checked.announces(), announces.as_slice());
    }

    #[test]
    fn try_into_checked_rejects_empty_response() {
        let request = AnnouncesRequest {
            head: HashOf::zero(),
            until: AnnouncesRequestUntil::ChainLen(1.try_into().unwrap()),
        };

        let err = AnnouncesResponse {
            announces: Vec::new(),
        }
        .try_into_checked(request)
        .unwrap_err();

        match err {
            AnnouncesResponseError::Empty => {}
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn try_into_checked_rejects_head_mismatch() {
        let announces = make_chain(2);
        let actual_head = announces.last().unwrap().to_hash();
        let wrong_head = HashOf::zero();
        let tail_hash = announces.first().unwrap().to_hash();

        let response = AnnouncesResponse { announces };

        let err = response
            .try_into_checked(AnnouncesRequest {
                head: wrong_head,
                until: AnnouncesRequestUntil::Tail(tail_hash),
            })
            .unwrap_err();

        match err {
            AnnouncesResponseError::HeadMismatch { expected, received } => {
                assert_eq!(expected, wrong_head);
                assert_eq!(received, actual_head);
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn try_into_checked_rejects_tail_mismatch() {
        let announces = make_chain(3);
        let actual_tail = announces.first().unwrap().to_hash();
        let head_hash = announces.last().unwrap().to_hash();
        let wrong_tail = HashOf::zero();

        let err = AnnouncesResponse {
            announces: announces.clone(),
        }
        .try_into_checked(AnnouncesRequest {
            head: head_hash,
            until: AnnouncesRequestUntil::Tail(wrong_tail),
        })
        .unwrap_err();

        match err {
            AnnouncesResponseError::TailMismatch { expected, received } => {
                assert_eq!(expected, wrong_tail);
                assert_eq!(received, actual_tail);
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn try_into_checked_rejects_len_mismatch() {
        let announces = make_chain(2);
        let head_hash = announces.last().unwrap().to_hash();

        let err = AnnouncesResponse { announces }
            .try_into_checked(AnnouncesRequest {
                head: head_hash,
                until: AnnouncesRequestUntil::ChainLen(3.try_into().unwrap()),
            })
            .unwrap_err();

        match err {
            AnnouncesResponseError::LenMismatch { expected, received } => {
                assert_eq!(expected, 3);
                assert_eq!(received, 2);
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn try_into_checked_rejects_non_linked_chain() {
        let mut announces = make_chain(3);
        announces[1].parent = HashOf::zero();
        let head_hash = announces.last().unwrap().to_hash();
        let tail_hash = announces.first().unwrap().to_hash();

        let err = AnnouncesResponse { announces }
            .try_into_checked(AnnouncesRequest {
                head: head_hash,
                until: AnnouncesRequestUntil::Tail(tail_hash),
            })
            .unwrap_err();

        match err {
            AnnouncesResponseError::ChainIsNotLinked => {}
            other => panic!("unexpected error: {other:?}"),
        }
    }
}
