// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Per-peer proposal-part stream reassembly.
//!
//! Malachite chunks each proposal into a sequence of `StreamMessage`s
//! (Init, one or more Data, Fin). [`PartStreamsMap`] keeps the per-
//! `(peer_id, stream_id)` reassembly buffer and, once a stream is
//! complete, returns the assembled [`ProposalParts`] in sequence
//! order.

use std::{
    cmp::Ordering,
    collections::{BTreeMap, BTreeSet, BinaryHeap, HashSet},
};

use parity_scale_codec::{Decode, Encode, Error as CodecError, Input, Output};

use malachitebft_app_channel::app::{
    streaming::{Sequence, StreamId, StreamMessage},
    types::{PeerId, core::Round},
};

use crate::{
    context::{Height, ProposalInit, ProposalPart},
    types::Address,
};

const MAX_STREAM_MESSAGES: u64 = 16;
const MAX_STREAMS_PER_PEER: usize = 64;
const MAX_STREAMS_TOTAL: usize = 1024;

type StreamKey = (PeerId, StreamId);

/// Min-heap wrapper that orders `StreamMessage`s by ascending sequence.
struct MinSeq<T>(StreamMessage<T>);

impl<T> PartialEq for MinSeq<T> {
    fn eq(&self, other: &Self) -> bool {
        self.0.sequence == other.0.sequence
    }
}

impl<T> Eq for MinSeq<T> {}

impl<T> Ord for MinSeq<T> {
    fn cmp(&self, other: &Self) -> Ordering {
        // BinaryHeap is a max-heap; reverse to get min-by-sequence.
        other.0.sequence.cmp(&self.0.sequence)
    }
}

impl<T> PartialOrd for MinSeq<T> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

struct MinHeap<T>(BinaryHeap<MinSeq<T>>);

impl<T> Default for MinHeap<T> {
    fn default() -> Self {
        Self(BinaryHeap::new())
    }
}

impl<T> MinHeap<T> {
    fn push(&mut self, msg: StreamMessage<T>) {
        self.0.push(MinSeq(msg));
    }

    fn len(&self) -> usize {
        self.0.len()
    }

    fn drain(&mut self) -> Vec<T> {
        let mut out = Vec::with_capacity(self.0.len());
        while let Some(MinSeq(msg)) = self.0.pop() {
            if let Some(data) = msg.content.into_data() {
                out.push(data);
            }
        }
        out
    }
}

#[derive(Default)]
struct StreamState {
    buffer: MinHeap<ProposalPart>,
    init_info: Option<ProposalInit>,
    seen_sequences: HashSet<Sequence>,
    total_messages: Option<usize>,
}

impl StreamState {
    fn is_done(&self) -> bool {
        self.init_info.is_some()
            && self
                .total_messages
                .is_some_and(|total| self.buffer.len() == total)
    }

    fn insert(&mut self, msg: StreamMessage<ProposalPart>) -> Option<ProposalParts> {
        if msg.is_first() {
            self.init_info = msg.content.as_data().and_then(|p| p.as_init()).cloned();
        }
        if msg.is_fin() {
            self.total_messages = Some(msg.sequence as usize + 1);
        }
        self.buffer.push(msg);
        if self.is_done() {
            let init_info = self.init_info.take()?;
            Some(ProposalParts {
                height: init_info.height,
                round: init_info.round,
                proposer: init_info.proposer,
                parts: self.buffer.drain(),
            })
        } else {
            None
        }
    }
}

/// Fully reassembled proposal — what [`PartStreamsMap`] hands back
/// to the caller once an entire stream has arrived.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProposalParts {
    pub height: Height,
    pub round: Round,
    pub proposer: Address,
    pub parts: Vec<ProposalPart>,
}

impl Encode for ProposalParts {
    fn encode_to<W: Output + ?Sized>(&self, dest: &mut W) {
        self.height.as_u64().encode_to(dest);
        // `Round` doesn't have a native SCALE impl; reuse the i64
        // mapping the malachite-side codec uses.
        self.round.as_i64().encode_to(dest);
        self.proposer.0.0.encode_to(dest);
        self.parts.encode_to(dest);
    }
}

impl Decode for ProposalParts {
    fn decode<I: Input>(input: &mut I) -> Result<Self, CodecError> {
        let height = Height::new(u64::decode(input)?);
        let round_raw = i64::decode(input)?;
        let round = if round_raw == -1 {
            Round::Nil
        } else if round_raw >= 0 && round_raw <= u32::MAX as i64 {
            Round::new(round_raw as u32)
        } else {
            return Err(CodecError::from("Round out of range in ProposalParts"));
        };
        let proposer_bytes = <[u8; 20]>::decode(input)?;
        let proposer = Address::from_inner(gsigner::schemes::secp256k1::Address(proposer_bytes));
        let parts = Vec::<ProposalPart>::decode(input)?;
        Ok(Self {
            height,
            round,
            proposer,
            parts,
        })
    }
}

impl ProposalParts {
    pub fn init(&self) -> Option<&ProposalInit> {
        self.parts.iter().find_map(|p| p.as_init())
    }

    pub fn data_block_bytes(&self) -> Option<&[u8]> {
        self.parts
            .iter()
            .find_map(|p| p.as_data())
            .map(|d| d.block_bytes.as_slice())
    }
}

#[derive(Default)]
pub struct PartStreamsMap {
    streams: BTreeMap<StreamKey, StreamState>,
    peer_streams: BTreeMap<PeerId, usize>,
    recencies: BTreeMap<StreamKey, u64>,
    recency_order: BTreeSet<(u64, PeerId, StreamId)>,
    next_recency: u64,
}

impl PartStreamsMap {
    pub fn new() -> Self {
        Self::default()
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.streams.len()
    }

    /// Insert a part. Returns `Some(parts)` once the stream is
    /// complete (all parts seen + Fin received). Subsequent calls for
    /// the same `(peer, stream)` after completion return `None` — the
    /// state has been removed.
    pub fn insert(
        &mut self,
        peer_id: PeerId,
        msg: StreamMessage<ProposalPart>,
    ) -> Option<ProposalParts> {
        let key = (peer_id, msg.stream_id.clone());
        if msg.sequence >= MAX_STREAM_MESSAGES {
            self.remove_stream(&key);
            return None;
        }
        if !self.streams.contains_key(&key) {
            self.make_room_for_new_stream(peer_id);
        }

        let result = {
            let state = self.streams.entry(key.clone()).or_insert_with(|| {
                *self.peer_streams.entry(peer_id).or_default() += 1;
                StreamState::default()
            });
            if !state.seen_sequences.insert(msg.sequence) {
                return None;
            }
            state.insert(msg)
        };
        if result.is_some() {
            self.remove_stream(&key);
        } else {
            self.touch_stream(&key);
        }
        result
    }

    fn make_room_for_new_stream(&mut self, peer_id: PeerId) {
        if self
            .peer_streams
            .get(&peer_id)
            .is_some_and(|count| *count >= MAX_STREAMS_PER_PEER)
            && let Some(key) = self.oldest_stream_for_peer(peer_id)
        {
            self.remove_stream(&key);
        }

        if self.streams.len() >= MAX_STREAMS_TOTAL
            && let Some((_, peer_id, stream_id)) = self.recency_order.iter().next().cloned()
        {
            self.remove_stream(&(peer_id, stream_id));
        }
    }

    fn oldest_stream_for_peer(&self, peer_id: PeerId) -> Option<StreamKey> {
        self.recency_order
            .iter()
            .find(|(_, candidate_peer, _)| *candidate_peer == peer_id)
            .map(|(_, peer_id, stream_id)| (*peer_id, stream_id.clone()))
    }

    fn touch_stream(&mut self, key: &StreamKey) {
        let recency = self.next_recency;
        self.next_recency = self
            .next_recency
            .checked_add(1)
            .expect("stream recency counter overflowed");

        if let Some(old_recency) = self.recencies.insert(key.clone(), recency) {
            self.recency_order
                .remove(&(old_recency, key.0, key.1.clone()));
        }
        self.recency_order.insert((recency, key.0, key.1.clone()));
    }

    fn remove_stream(&mut self, key: &StreamKey) {
        if self.streams.remove(key).is_none() {
            return;
        }

        if let Some(recency) = self.recencies.remove(key) {
            self.recency_order.remove(&(recency, key.0, key.1.clone()));
        }

        if let Some(count) = self.peer_streams.get_mut(&key.0) {
            *count -= 1;
            if *count == 0 {
                self.peer_streams.remove(&key.0);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        context::{ProposalData, ProposalInit},
        signing::{MalachiteSigner, private_key_from_bytes},
    };
    use malachitebft_app_channel::app::streaming::StreamContent;

    fn peer_id(byte: u8) -> PeerId {
        let mut bytes = [0u8; 32];
        bytes[31] = byte;
        let lp = crate::signing::libp2p_peer_id(&bytes);
        PeerId::from_bytes(&lp.to_bytes()).expect("libp2p peer-id is valid multihash")
    }

    fn sid(h: u64) -> StreamId {
        StreamId::new(h.to_be_bytes().to_vec().into())
    }

    fn init_part(h: u64) -> ProposalPart {
        let mut bytes = [0u8; 32];
        bytes[31] = 1;
        let signer = MalachiteSigner::new(private_key_from_bytes(&bytes).unwrap());
        let pk = signer.public_key();
        ProposalPart::Init(ProposalInit::new(
            Height::new(h),
            Round::new(0),
            Round::Nil,
            Address::from_public_key(&pk),
        ))
    }

    fn data_part(payload: &[u8]) -> ProposalPart {
        ProposalPart::Data(ProposalData::new(payload.to_vec()))
    }

    fn msg(stream_id: StreamId, seq: u64, content: ProposalPart) -> StreamMessage<ProposalPart> {
        StreamMessage::new(stream_id, seq, StreamContent::Data(content))
    }

    fn fin_msg(stream_id: StreamId, seq: u64) -> StreamMessage<ProposalPart> {
        StreamMessage::new(stream_id, seq, StreamContent::Fin)
    }

    fn fill_global_cap(map: &mut PartStreamsMap, start_stream: u64) {
        let mut stream = start_stream;
        for peer_byte in 2..=250 {
            for _ in 0..MAX_STREAMS_PER_PEER {
                if map.len() == MAX_STREAMS_TOTAL {
                    return;
                }
                let p = peer_id(peer_byte);
                let s = sid(stream);
                assert!(map.insert(p, msg(s, 0, init_part(stream))).is_none());
                stream += 1;
            }
        }
        assert_eq!(map.len(), MAX_STREAMS_TOTAL);
    }

    #[test]
    fn complete_in_order_assembles() {
        let mut map = PartStreamsMap::new();
        let p = peer_id(1);
        let s = sid(1);

        assert!(map.insert(p, msg(s.clone(), 0, init_part(1))).is_none());
        assert!(
            map.insert(p, msg(s.clone(), 1, data_part(b"hello")))
                .is_none()
        );
        let done = map.insert(p, fin_msg(s.clone(), 2)).unwrap();
        assert_eq!(done.height, Height::new(1));
        assert_eq!(done.parts.len(), 2);
        assert_eq!(done.data_block_bytes(), Some(&b"hello"[..]));
    }

    #[test]
    fn completed_stream_releases_slot() {
        let mut map = PartStreamsMap::new();
        let p = peer_id(1);
        let s = sid(11);

        assert!(map.insert(p, msg(s.clone(), 0, init_part(11))).is_none());
        assert!(
            map.insert(p, msg(s.clone(), 1, data_part(b"done")))
                .is_none()
        );
        assert!(map.insert(p, fin_msg(s.clone(), 2)).is_some());

        assert!(
            !map.streams.contains_key(&(p, s)),
            "completed stream must be removed from PartStreamsMap"
        );
        assert_eq!(map.streams.len(), 0);
        assert!(map.recencies.is_empty());
        assert!(map.recency_order.is_empty());
    }

    #[test]
    fn complete_out_of_order_assembles() {
        let mut map = PartStreamsMap::new();
        let p = peer_id(1);
        let s = sid(2);
        // Fin arrives before Data and Init.
        assert!(map.insert(p, fin_msg(s.clone(), 2)).is_none());
        assert!(
            map.insert(p, msg(s.clone(), 1, data_part(b"world")))
                .is_none()
        );
        let done = map.insert(p, msg(s.clone(), 0, init_part(2))).unwrap();
        assert_eq!(done.parts.len(), 2);
        assert_eq!(done.data_block_bytes(), Some(&b"world"[..]));
    }

    #[test]
    fn duplicate_sequence_is_ignored() {
        let mut map = PartStreamsMap::new();
        let p = peer_id(1);
        let s = sid(3);
        assert!(map.insert(p, msg(s.clone(), 0, init_part(3))).is_none());
        // Same sequence again.
        assert!(map.insert(p, msg(s.clone(), 0, init_part(3))).is_none());
    }

    #[test]
    fn distinct_streams_are_independent() {
        let mut map = PartStreamsMap::new();
        let p = peer_id(1);
        let s1 = sid(10);
        let s2 = sid(20);
        assert!(map.insert(p, msg(s1.clone(), 0, init_part(10))).is_none());
        assert!(map.insert(p, msg(s2.clone(), 0, init_part(20))).is_none());
        assert!(map.insert(p, msg(s1.clone(), 1, data_part(b"a"))).is_none());
        assert!(map.insert(p, fin_msg(s1.clone(), 2)).is_some());
        // Stream s2 still pending.
        assert!(map.insert(p, fin_msg(s2.clone(), 2)).is_none());
    }

    #[test]
    fn per_peer_cap_evicts_oldest_and_accepts_new_stream() {
        let mut map = PartStreamsMap::new();
        let p = peer_id(1);

        let first = sid(0xA000_0000);
        for stream_idx in 0..MAX_STREAMS_PER_PEER as u64 {
            let s = sid(0xA000_0000 + stream_idx);
            assert!(map.insert(p, msg(s.clone(), 0, init_part(1))).is_none());
        }

        assert_eq!(map.streams.len(), MAX_STREAMS_PER_PEER);
        assert_eq!(map.peer_streams.get(&p), Some(&MAX_STREAMS_PER_PEER));

        let newest = sid(0xB000_0000);
        assert!(
            map.insert(p, msg(newest.clone(), 0, init_part(1)))
                .is_none()
        );

        assert_eq!(map.streams.len(), MAX_STREAMS_PER_PEER);
        assert!(!map.streams.contains_key(&(p, first)));
        assert!(map.streams.contains_key(&(p, newest.clone())));

        assert!(
            map.insert(p, msg(newest.clone(), 1, data_part(b"new")))
                .is_none()
        );
        assert!(map.insert(p, fin_msg(newest.clone(), 2)).is_some());
        assert!(!map.streams.contains_key(&(p, newest)));
        assert_eq!(map.peer_streams.get(&p), Some(&(MAX_STREAMS_PER_PEER - 1)));
    }

    #[test]
    fn global_cap_evicts_oldest_and_stays_bounded() {
        let mut map = PartStreamsMap::new();
        let first_peer = peer_id(1);
        let first_stream = sid(10);

        assert!(
            map.insert(first_peer, msg(first_stream.clone(), 0, init_part(10)))
                .is_none()
        );
        fill_global_cap(&mut map, 1_000);
        assert_eq!(map.len(), MAX_STREAMS_TOTAL);

        let new_peer = peer_id(251);
        let new_stream = sid(20_000);
        assert!(
            map.insert(new_peer, msg(new_stream.clone(), 0, init_part(20_000)))
                .is_none()
        );

        assert_eq!(map.len(), MAX_STREAMS_TOTAL);
        assert!(!map.streams.contains_key(&(first_peer, first_stream)));
        assert!(map.streams.contains_key(&(new_peer, new_stream)));
    }

    #[test]
    fn valid_parts_refresh_existing_stream_recency() {
        let mut map = PartStreamsMap::new();
        let p = peer_id(1);
        let refreshed = sid(50);
        let stale = sid(51);

        assert!(
            map.insert(p, msg(refreshed.clone(), 0, init_part(50)))
                .is_none()
        );
        assert!(
            map.insert(p, msg(stale.clone(), 0, init_part(51)))
                .is_none()
        );
        assert!(
            map.insert(p, msg(refreshed.clone(), 1, data_part(b"fresh")))
                .is_none()
        );

        fill_global_cap(&mut map, 2_000);

        let new_peer = peer_id(251);
        let new_stream = sid(30_000);
        assert!(
            map.insert(new_peer, msg(new_stream.clone(), 0, init_part(30_000)))
                .is_none()
        );

        assert!(map.streams.contains_key(&(p, refreshed)));
        assert!(!map.streams.contains_key(&(p, stale)));
        assert!(map.streams.contains_key(&(new_peer, new_stream)));
    }

    #[test]
    fn malformed_far_future_fin_evicts_only_its_stream() {
        let mut map = PartStreamsMap::new();
        let p = peer_id(1);
        let s = sid(30);
        let other = sid(31);

        assert!(map.insert(p, msg(s.clone(), 0, init_part(30))).is_none());
        assert!(
            map.insert(p, msg(other.clone(), 0, init_part(31)))
                .is_none()
        );
        assert!(map.streams.contains_key(&(p, s.clone())));
        assert!(
            map.insert(p, fin_msg(s.clone(), MAX_STREAM_MESSAGES))
                .is_none()
        );

        assert!(!map.streams.contains_key(&(p, s)));
        assert!(map.streams.contains_key(&(p, other.clone())));
        assert_eq!(map.peer_streams.get(&p), Some(&1));
        assert!(map.recencies.contains_key(&(p, other)));
    }

    #[test]
    fn completed_stream_releases_slot_after_cap_eviction() {
        let mut map = PartStreamsMap::new();
        let p = peer_id(1);
        let first = sid(40);

        assert!(
            map.insert(p, msg(first.clone(), 0, init_part(40)))
                .is_none()
        );
        for stream_idx in 1..MAX_STREAMS_PER_PEER as u64 {
            let s = sid(40 + stream_idx);
            assert!(map.insert(p, msg(s, 0, init_part(40))).is_none());
        }
        assert_eq!(map.streams.len(), MAX_STREAMS_PER_PEER);

        let over_cap = sid(10_000);
        assert!(
            map.insert(p, msg(over_cap.clone(), 0, init_part(40)))
                .is_none()
        );
        assert!(!map.streams.contains_key(&(p, first.clone())));
        assert!(map.streams.contains_key(&(p, over_cap.clone())));

        assert!(
            map.insert(p, msg(over_cap.clone(), 1, data_part(b"ok")))
                .is_none()
        );
        assert!(map.insert(p, fin_msg(over_cap.clone(), 2)).is_some());
        assert!(!map.streams.contains_key(&(p, over_cap)));
        assert_eq!(map.streams.len(), MAX_STREAMS_PER_PEER - 1);
    }
}
