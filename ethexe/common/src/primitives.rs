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

use core::num::NonZeroU32;

use crate::{DEFAULT_BLOCK_GAS_LIMIT, ToDigest, events::BlockEvent};
use alloc::{
    collections::{btree_map::BTreeMap, btree_set::BTreeSet},
    vec::Vec,
};
use gear_core::{ids::prelude::CodeIdExt as _, utils};
use gprimitives::{ActorId, CodeId, H256, MessageId};
use parity_scale_codec::{Decode, Encode};
use sha3::Digest as _;

pub type ProgramStates = BTreeMap<ActorId, StateHashWithQueueSize>;

// TODO #4875: use HashOf here
#[derive(
    Debug,
    Clone,
    Copy,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Encode,
    Decode,
    derive_more::Deref,
    derive_more::DerefMut,
    derive_more::Display,
)]
#[display("{}", self.0)]
pub struct AnnounceHash(pub H256);

impl AnnounceHash {
    pub const fn zero() -> Self {
        Self(H256::zero())
    }

    #[cfg(feature = "std")]
    pub fn random() -> Self {
        Self(H256::random())
    }
}

#[derive(Debug, Clone, Copy, Default, Encode, Decode, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct BlockHeader {
    pub height: u32,
    pub timestamp: u64,
    pub parent_hash: H256,
}

impl BlockHeader {
    pub fn dummy(height: u32) -> Self {
        let mut parent_hash = [0; 32];
        parent_hash[..4].copy_from_slice(&height.to_le_bytes());

        Self {
            height,
            timestamp: height as u64 * 12,
            parent_hash: parent_hash.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockData {
    pub hash: H256,
    pub header: BlockHeader,
    pub events: Vec<BlockEvent>,
}

impl BlockData {
    pub fn to_simple(&self) -> SimpleBlockData {
        SimpleBlockData {
            hash: self.hash,
            header: self.header,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimpleBlockData {
    pub hash: H256,
    pub header: BlockHeader,
}

#[derive(Clone, Debug, Encode, Decode, PartialEq, Eq, Hash)]
pub struct Announce {
    pub block_hash: H256,
    pub parent: AnnounceHash,
    pub gas_allowance: Option<u64>,
    pub off_chain_transactions: Vec<H256>,
}

impl Announce {
    pub fn to_hash(&self) -> AnnounceHash {
        AnnounceHash(H256(utils::hash(&self.encode())))
    }

    pub fn base(block_hash: H256, parent: AnnounceHash) -> Self {
        Self {
            block_hash,
            parent,
            gas_allowance: None,
            off_chain_transactions: Vec::new(),
        }
    }

    pub fn with_default_gas(block_hash: H256, parent: AnnounceHash) -> Self {
        Self {
            block_hash,
            parent,
            gas_allowance: Some(DEFAULT_BLOCK_GAS_LIMIT),
            off_chain_transactions: Vec::new(),
        }
    }

    pub fn is_base(&self) -> bool {
        self.gas_allowance.is_none() && self.off_chain_transactions.is_empty()
    }
}

impl ToDigest for Announce {
    fn update_hasher(&self, hasher: &mut sha3::Keccak256) {
        hasher.update(self.block_hash);
        hasher.update(self.gas_allowance.encode());
        hasher.update(self.off_chain_transactions.encode());
    }
}

#[derive(PartialEq, Eq, Hash, Debug, Clone, Copy, Default, Encode, Decode)]
#[cfg_attr(feature = "std", derive(serde::Serialize))]
pub struct StateHashWithQueueSize {
    pub hash: H256,
    pub cached_queue_size: u8,
}

impl StateHashWithQueueSize {
    pub fn zero() -> Self {
        Self {
            hash: H256::zero(),
            cached_queue_size: 0,
        }
    }
}

#[derive(Debug, Clone, Default, Encode, Decode, PartialEq, Eq)]
pub struct CodeBlobInfo {
    pub timestamp: u64,
    pub tx_hash: H256,
}

#[derive(Clone, PartialEq, Eq, derive_more::Debug)]
pub struct CodeAndIdUnchecked {
    #[debug("{:#x} bytes", code.len())]
    pub code: Vec<u8>,
    pub code_id: CodeId,
}

#[derive(Clone, PartialEq, Eq, derive_more::Debug)]
pub struct CodeAndId {
    #[debug("{:#x} bytes", code.len())]
    code: Vec<u8>,
    code_id: CodeId,
}

impl CodeAndId {
    pub fn new(code: Vec<u8>) -> Self {
        let code_id = CodeId::generate(&code);
        Self { code, code_id }
    }

    pub fn code(&self) -> &[u8] {
        &self.code
    }

    pub fn code_id(&self) -> CodeId {
        self.code_id
    }

    /// Creates a new `CodeAndId` from an unchecked version, asserting that the `code_id` matches the generated one.
    /// # Panics
    ///
    /// If the `code_id` does not match the generated one from the `code`, this function will panic.
    pub fn from_unchecked(code_and_id: CodeAndIdUnchecked) -> Self {
        let CodeAndIdUnchecked { code, code_id } = code_and_id;
        assert_eq!(
            code_id,
            CodeId::generate(&code),
            "CodeId does not match the provided code"
        );
        Self { code, code_id }
    }

    pub fn into_unchecked(self) -> CodeAndIdUnchecked {
        CodeAndIdUnchecked {
            code: self.code,
            code_id: self.code_id,
        }
    }
}

/// RemoveFromMailbox key; (msgs sources program (mailbox and queue provider), destination user id)
pub type Rfm = (ActorId, ActorId);

/// SendDispatch key; (msgs destinations program (stash and queue provider), message id)
pub type Sd = (ActorId, MessageId);

/// SendUserMessage key; (msgs sources program (mailbox and stash provider))
pub type Sum = ActorId;

/// NOTE: generic keys differs to Vara and have been chosen dependent on storage organization of ethexe.
pub type ScheduledTask = gear_core::tasks::ScheduledTask<Rfm, Sd, Sum>;

/// Scheduler; (block height, scheduled task)
pub type Schedule = BTreeMap<u32, BTreeSet<ScheduledTask>>;

/// Until condition for announces request (see [`AnnouncesRequest`]).
#[derive(PartialEq, Eq, Hash, Debug, Clone, Copy, Encode, Decode)]
pub enum AnnouncesRequestUntil {
    /// Request until a specific tail announce hash
    Tail(AnnounceHash),
    /// Request until a specific chain length
    ChainLen(NonZeroU32),
}

/// Request announces body (see [`Announce`]) chain from `head_announce_hash` to `tail_announce_hash`.
/// `tail_announce_hash` must not be included in response.
/// If `tail_announce_hash` is None, then only data `head_announce_hash` must be returned.
#[derive(PartialEq, Eq, Hash, Debug, Clone, Copy, Encode, Decode)]
pub struct AnnouncesRequest {
    /// Hash of the requested chain head announce
    pub head: AnnounceHash,
    /// Request until this condition is met
    pub until: AnnouncesRequestUntil,
}

// TODO +_+_+: can be optimized - no reasons to return all announces in chain,
// only not-base announces could be returned.
/// Response for announces request.
/// Must contain all announces for the requested range.
/// Must be sorted from predecessors to successors.
#[derive(PartialEq, Eq, Hash, Debug, Clone, Default, Encode, Decode)]
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
        expected: AnnounceHash,
        received: AnnounceHash,
    },
    #[display("announces tail mismatch, expected hash {expected}, received {received}")]
    TailMismatch {
        expected: AnnounceHash,
        received: AnnounceHash,
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

    fn make_chain(len: usize) -> Vec<Announce> {
        assert!(len > 0);
        let mut chain = Vec::with_capacity(len);
        let mut parent = AnnounceHash::zero();

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
            head: AnnounceHash::zero(),
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
        let wrong_head = AnnounceHash::zero();
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
        let wrong_tail = AnnounceHash::zero();

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
        announces[1].parent = AnnounceHash::zero();
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
