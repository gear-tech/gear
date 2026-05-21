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
    collections::{BTreeMap, BinaryHeap, HashSet},
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
    total_messages: usize,
    fin_received: bool,
}

impl StreamState {
    fn is_done(&self) -> bool {
        self.init_info.is_some() && self.fin_received && self.buffer.len() == self.total_messages
    }

    fn insert(&mut self, msg: StreamMessage<ProposalPart>) -> Option<ProposalParts> {
        if msg.is_first() {
            self.init_info = msg.content.as_data().and_then(|p| p.as_init()).cloned();
        }
        if msg.is_fin() {
            self.fin_received = true;
            self.total_messages = msg.sequence as usize + 1;
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

// TODO: #5473 `PartStreamsMap` has no per-peer cap, no total cap, and no
// eviction for streams that never receive a valid `Fin`. Pinned by the
// (ignored) regression test
// `streaming::tests::part_streams_map_grows_unbounded_under_fin_sequence_attack`.
#[derive(Default)]
pub struct PartStreamsMap {
    streams: BTreeMap<(PeerId, StreamId), StreamState>,
}

impl PartStreamsMap {
    pub fn new() -> Self {
        Self::default()
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
        let stream_id = msg.stream_id.clone();
        let state = self
            .streams
            .entry((peer_id, stream_id.clone()))
            .or_default();
        if !state.seen_sequences.insert(msg.sequence) {
            return None;
        }
        let result = state.insert(msg);
        if state.is_done() {
            self.streams.remove(&(peer_id, stream_id));
        }
        result
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

    /// REPRODUCES: a single peer can grow `PartStreamsMap` without
    /// bound by either (a) opening fresh `stream_id`s and never sending
    /// `Fin`, or (b) sending a `Fin` with a `sequence` far above any
    /// part it actually delivers so the `total_messages == buffer.len()`
    /// gate is unreachable.
    #[test]
    #[ignore = "tracks issue #5473 in streaming.rs: unbounded PartStreamsMap"]
    fn part_streams_map_grows_unbounded_under_fin_sequence_attack() {
        let mut map = PartStreamsMap::new();
        let p = peer_id(1);

        // Attack A: a peer opens many streams and never finalises.
        // 100 distinct stream_ids, each with Init + Data but no Fin.
        for stream_idx in 0..100u64 {
            let s = sid(0xA000_0000 + stream_idx);
            assert!(map.insert(p, msg(s.clone(), 0, init_part(1))).is_none());
            assert!(map.insert(p, msg(s, 1, data_part(b"x"))).is_none());
        }

        // Attack B: cheaper still — one message per stream, Fin with a
        // far-future sequence. `total_messages` becomes
        // `u64::MAX as usize + 1` (wraps to 0 in release, panics in
        // debug), but the `is_done` gate `buffer.len() == total_messages`
        // is unreachable for any sane traffic. 100 more streams.
        for stream_idx in 0..100u64 {
            let s = sid(0xB000_0000 + stream_idx);
            assert!(map.insert(p, fin_msg(s, u64::MAX / 2)).is_none());
        }

        // Desired behaviour: a single peer cannot hold > a bounded
        // number of in-flight stream slots. The exact cap is up to the
        // fix, but it must be much smaller than the 200 we just pushed.
        assert!(
            map.streams.len() < 200,
            "PartStreamsMap grew to {} entries under a single-peer flood — \
             needs per-peer cap + GC for never-finalised / bogus-Fin streams",
            map.streams.len(),
        );
    }

    /// REPRODUCES: `StreamState::insert` couples Init-extraction with
    /// `msg.is_first()` (= `sequence == 0`). If a peer puts a `Data`
    /// part at sequence 0 and the actual `Init` at sequence 1, the
    /// `is_first()` branch fires for the Data — `as_init()` returns
    /// `None` — so `init_info` stays `None` forever. When the proper
    /// `Init` arrives at sequence 1 it's filed into the buffer as a
    /// regular data part (no special handling), `init_info` is never
    /// populated, and `is_done()`'s `init_info.is_some()` gate can
    /// never succeed even after every part + Fin arrives.
    ///
    /// Concrete consequence: the `(peer_id, stream_id)` slot is held
    /// indefinitely — `PartStreamsMap::insert` removes the entry only
    /// when `state.is_done()` returns true. A single malicious peer
    /// can hold one stuck slot per stream they open (and open
    /// arbitrarily many slots — see #5473 for the broader cap issue).
    ///
    /// Expected fix: either (a) extract the Init from whichever
    /// `ProposalPart::Init` arrives, regardless of its sequence
    /// position, or (b) reject any sequence-0 message whose content
    /// is not a `ProposalPart::Init` as a protocol violation so the
    /// state is dropped immediately rather than left in a permanently
    /// non-completable shape.
    ///
    /// This pins (a): a stream with Data@0, Init@1, Data@2, Fin@3
    /// must still assemble — every part the proposer intended is
    /// present; the only oddity is the (peer-controlled) ordering of
    /// the Init part within the sequence space.
    #[test]
    #[ignore = "tracks bug: StreamState ties Init extraction to sequence==0, stuck stream when Init isn't first"]
    fn init_at_non_zero_sequence_never_completes() {
        let mut map = PartStreamsMap::new();
        let p = peer_id(8);
        let s = sid(0xBADF00D);

        // Sequence 0: a Data part (not Init). `is_first()` true, but
        // `as_init()` on a Data returns None — init_info stays None.
        assert!(
            map.insert(p, msg(s.clone(), 0, data_part(b"AAAA")))
                .is_none(),
        );
        // Sequence 1: the actual Init. `is_first()` false — Init is
        // filed as a plain buffered part, init_info never updated.
        assert!(map.insert(p, msg(s.clone(), 1, init_part(99))).is_none());
        // Sequence 2: another data part.
        assert!(
            map.insert(p, msg(s.clone(), 2, data_part(b"BBBB")))
                .is_none(),
        );
        // Fin at sequence 3 — total_messages = 4, buffer.len() = 4.
        // `is_done()` would fire IF `init_info` was set. Currently it
        // isn't — so the stream is stuck.
        let done = map.insert(p, fin_msg(s.clone(), 3));

        assert!(
            done.is_some(),
            "stream with Init at sequence > 0 must still assemble: \
             the proposer placed Init + Data + Fin and the buffer has \
             all 4 parts, but `init_info` was never populated because \
             `is_first()` (= sequence == 0) saw the Data part instead. \
             StreamState should extract Init by content kind, not by \
             sequence position — otherwise a malicious peer can hold a \
             PartStreamsMap slot indefinitely with a single 4-message \
             stream (compounding the no-cap issue in #5473).",
        );
    }

    /// REPRODUCES: a malicious sender can prematurely complete a
    /// proposal stream by sending two `Fin` messages with different
    /// sequences. `StreamState::insert` unconditionally overwrites
    /// `total_messages = msg.sequence as usize + 1` on every `Fin`,
    /// so a second `Fin` at a lower sequence lowers the completion
    /// target. By choosing the second `Fin`'s sequence to equal the
    /// current `buffer.len()`, the attacker forces
    /// `is_done` (`buffer.len() == total_messages`) to fire even though
    /// the proposer's original intent (encoded in the FIRST `Fin`)
    /// was a much larger part count.
    ///
    /// Expected behaviour: once a `Fin` has been seen, a later `Fin`
    /// at a different sequence must be rejected — `total_messages`
    /// should be locked, or the second `Fin` should mark the stream
    /// as corrupted and drop the state.
    #[test]
    #[ignore = "tracks double-Fin-sequence completion bug in streaming.rs"]
    fn double_fin_with_smaller_sequence_completes_stream_prematurely() {
        let mut map = PartStreamsMap::new();
        let p = peer_id(7);
        let s = sid(0xD00D);

        // Attacker plays the role of the proposer for (p, s). They
        // send a partial proposal: Init + three Data parts.
        assert!(map.insert(p, msg(s.clone(), 0, init_part(42))).is_none());
        assert!(
            map.insert(p, msg(s.clone(), 1, data_part(b"AAAA")))
                .is_none(),
        );
        assert!(
            map.insert(p, msg(s.clone(), 2, data_part(b"BBBB")))
                .is_none(),
        );
        assert!(
            map.insert(p, msg(s.clone(), 3, data_part(b"CCCC")))
                .is_none(),
        );
        // First `Fin` at sequence 100 — the proposer "intends" 101
        // parts in this stream. buffer.len() = 5 ≠ total = 101 ⇒ not
        // done.
        assert!(map.insert(p, fin_msg(s.clone(), 100)).is_none());

        // Malicious second `Fin` at sequence 5. `seen_sequences`
        // doesn't yet contain 5, so it's accepted. The bug:
        // `total_messages` is overwritten to 6, and the buffer grows
        // to 6 entries (Init + 3 Data + 2 Fins). 6 == 6 ⇒ DONE.
        let done = map.insert(p, fin_msg(s.clone(), 5));

        assert!(
            done.is_none(),
            "double-Fin attack: a second `Fin` lowered \
             `total_messages` so that `buffer.len() == total_messages` \
             fires early. The stream completed even though only 4 of \
             the proposer's intended 101 parts were delivered. \
             total_messages should be locked after the first Fin, or \
             the second Fin should be rejected as a protocol \
             violation.",
        );
    }
}
