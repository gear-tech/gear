// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Malachite block model shared by the consensus service
//! (`ethexe-malachite-core`), the consensus glue layer
//! (`ethexe-malachite`), and the executor (`ethexe-compute`).
//!
//! - [`MB`] is the consensus block envelope: `(parent_hash, height,
//!   payload, reserved)`. Its hash is [`HashOf<MB>`].
//! - [`BlockPayload`] is the opaque, versioned, size-capped wire
//!   payload carried by an MB. The application schema
//!   ([`Operations`]) lives inside [`BlockPayload`] as SCALE-encoded
//!   bytes.
//! - [`CompactMb`] is `MB` with the payload bytes replaced by
//!   `operations_hash` — what gets indexed in the ethexe DB once the
//!   matching [`Operations`] blob is in CAS.
//! - [`Operations`] is the application-level ordered list of
//!   [`Operation`]s that the executor consumes.
//!
//! Protocol evolution is additive: a new behaviour gets a new
//! [`Operation`] variant with the next free `#[repr(u32)]` discriminant
//! (existing discriminants and their payloads are frozen forever, so
//! every historical operation stays decodable). Which operations a
//! validator *accepts* in a fresh proposal is gated separately, on the
//! validator side — older operations can be retired from new blocks
//! without ever losing the ability to decode and replay them.
//!
//! These types live in `ethexe-common` (rather than inside
//! `ethexe-malachite`) so `ethexe-processor` can accept them without
//! depending on the consensus layer.

use crate::{EB, HashOf, injected::SignedInjectedTransaction};
use alloc::vec::Vec;
use anyhow::{Result, anyhow};
use derive_more::{Deref, DerefMut, IntoIterator};
use gear_core::limited::LimitedVec;
use gprimitives::H256;
use parity_scale_codec::{Decode, Encode};
use scale_info::TypeInfo;

#[cfg(feature = "std")]
use serde::{Deserialize, Serialize};

/// Per-block payload size cap.
///
/// The whole [`MB`] ships as a single gossipsub message: the proposer
/// streams it as one `Data` proposal part, and the value-sync path fetches
/// a finalized block in one request-response round. Malachite's
/// `pubsub_max_size` (the gossipsub `max_transmit_size`) defaults to
/// 4 MiB, so the encoded MB must stay well under that. The 1 MiB cap
/// leaves ~4x headroom under the transport ceiling.
pub const MAX_BLOCK_PAYLOAD_BYTES: usize = 1024 * 1024;

/// Current `BlockPayload::version` written by this code path.
///
/// Bump in lockstep with a wire-format change in how the application
/// interprets [`BlockPayload::bytes`]; decoders MUST tolerate seeing
/// versions strictly less than the current one but MAY reject newer
/// ones.
pub const BLOCK_PAYLOAD_VERSION: u16 = 0;

/// Versioned, size-capped block payload carried by an [`MB`].
///
/// The consensus service treats `bytes` as opaque. The ethexe
/// application schema lives inside as a SCALE-encoded [`Operations`]
/// — `version` exists so a future protocol bump can change that
/// encoding without breaking the [`MB`] wire shape.
#[derive(Clone, Debug, Default, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct BlockPayload {
    pub version: u16,
    pub bytes: LimitedVec<u8, MAX_BLOCK_PAYLOAD_BYTES>,
}

impl BlockPayload {
    /// Wrap raw application bytes at the current
    /// [`BLOCK_PAYLOAD_VERSION`]. Returns `Err` if `bytes` exceeds
    /// [`MAX_BLOCK_PAYLOAD_BYTES`].
    pub fn new(bytes: Vec<u8>) -> Result<Self> {
        let len = bytes.len();
        let bytes = LimitedVec::try_from(bytes).map_err(|_| {
            anyhow!("block payload exceeds {MAX_BLOCK_PAYLOAD_BYTES}-byte cap (got {len})")
        })?;
        Ok(Self {
            version: BLOCK_PAYLOAD_VERSION,
            bytes,
        })
    }

    /// Content-addressed hash of the application bytes (the value
    /// stored in [`CompactMb::operations_hash`]). The `version` prefix
    /// deliberately does NOT contribute to the digest: at v0 the
    /// bytes are SCALE-encoded [`Operations`], so this hash matches
    /// the legacy `Operations`-keyed CAS slot byte-for-byte.
    pub fn hash(&self) -> H256 {
        gear_core::utils::hash(self.bytes.as_ref()).into()
    }
}

/// Malachite block envelope: opaque versioned payload plus
/// chain-position fields (parent hash + height) and a [`Self::reserved`]
/// tail for future protocol extensions.
///
/// The block hash ([`Self::hash`]) is [`gear_core::utils::hash`]
/// (Blake2b-256) over a SCALE-encoded
/// `(parent_hash, height, payload_hash, reserved)` tuple, where
/// `payload_hash = BlockPayload::hash()`. Two nodes with the same
/// envelope content produce the same hash.
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct MB {
    pub parent_hash: HashOf<MB>,
    pub height: u64,
    pub payload: BlockPayload,
    pub reserved: [u8; 64],
}

impl MB {
    /// Construct an MB with `reserved` zeroed out.
    pub fn new(parent_hash: HashOf<MB>, height: u64, payload: BlockPayload) -> Self {
        Self {
            parent_hash,
            height,
            payload,
            reserved: [0u8; 64],
        }
    }

    /// Compute the canonical [`HashOf<MB>`] for this envelope.
    pub fn hash(&self) -> HashOf<MB> {
        let payload_hash = self.payload.hash();
        let inner = (self.parent_hash, self.height, payload_hash, self.reserved).encode();
        let raw: H256 = gear_core::utils::hash(&inner).into();
        // SAFETY: `raw` is the canonical MB envelope digest. Wrapping
        // it in `HashOf<MB>` is exactly what the constructor exists for.
        unsafe { HashOf::new(raw) }
    }
}

/// MB static identity. Same shape as [`MB`] but with the opaque
/// payload bytes replaced by `operations_hash`. Existence implies the
/// matching application-level [`Operations`] blob is in the
/// content-addressed half of the ethexe DB at `operations_hash`.
#[derive(
    Debug, Clone, Copy, Encode, Decode, TypeInfo, PartialEq, Eq, Hash, derive_more::Display,
)]
#[display("MB(height {height}, parent {parent}, operations_hash {operations_hash})")]
pub struct CompactMb {
    pub parent: HashOf<MB>,
    pub height: u64,
    pub operations_hash: H256,
    pub reserved: [u8; 64],
}

impl Default for CompactMb {
    fn default() -> Self {
        Self {
            parent: HashOf::zero(),
            height: 0,
            operations_hash: H256::zero(),
            reserved: [0u8; 64],
        }
    }
}

impl CompactMb {
    /// Recompute the [`HashOf<MB>`] from this compact record. Matches
    /// [`MB::hash`] byte-for-byte by construction (same SCALE tuple).
    pub fn mb_hash(&self) -> HashOf<MB> {
        let inner = (
            self.parent,
            self.height,
            self.operations_hash,
            self.reserved,
        )
            .encode();
        let raw: H256 = gear_core::utils::hash(&inner).into();
        // SAFETY: identical derivation to `MB::hash`.
        unsafe { HashOf::new(raw) }
    }
}

/// A single operation in the malachite block.
#[derive(Clone, Debug, PartialEq, Eq, TypeInfo, derive_more::IsVariant)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
#[repr(u32)]
pub enum Operation {
    /// Pin executor's view to a quarantine-passed Ethereum block.
    AdvanceTillEthereumBlock { block_hash: HashOf<EB> } = 0,

    /// Progress scheduled tasks (mailbox/waitlist/reservation cleanup).
    ProgressTasks = 1,

    /// Drain message queues within `gas_allowance`; producer emits last.
    ProcessQueues { gas_allowance: u64 } = 2,

    /// User-submitted transaction from the mempool.
    Injected(SignedInjectedTransaction) = 3,
}

impl Operation {
    /// The `u32` discriminant identifying this variant — the value written
    /// first by [`Encode`] and read back by [`Decode`].
    ///
    /// Discriminants are part of the consensus wire format: existing values
    /// are frozen forever (a new operation gets the next free number), so a
    /// node always decodes every historical operation it has ever seen.
    pub fn tag(&self) -> u32 {
        // Mirrors the `#[repr(u32)]` discriminants below and the `Decode`
        // arms. These three must agree; `operation_encoding_is_frozen` pins
        // the bytes so a divergence can't slip through.
        match self {
            Self::AdvanceTillEthereumBlock { .. } => 0,
            Self::ProgressTasks => 1,
            Self::ProcessQueues { .. } => 2,
            Self::Injected(_) => 3,
        }
    }
}

// Custom encoder/decoder so the discriminant is always a fixed-width `u32`
// tag, sidestepping parity-scale-codec's compact enum-index encoding (which
// only addresses up to 255 variants) and keeping room for many operations.

impl Decode for Operation {
    fn decode<I: parity_scale_codec::Input>(
        input: &mut I,
    ) -> core::result::Result<Self, parity_scale_codec::Error> {
        let tag = u32::decode(input)?;
        match tag {
            0 => Ok(Operation::AdvanceTillEthereumBlock {
                block_hash: HashOf::<EB>::decode(input)?,
            }),
            1 => Ok(Operation::ProgressTasks),
            2 => Ok(Operation::ProcessQueues {
                gas_allowance: u64::decode(input)?,
            }),
            3 => Ok(Operation::Injected(SignedInjectedTransaction::decode(
                input,
            )?)),
            _ => Err(parity_scale_codec::Error::from("invalid operation tag")),
        }
    }
}

impl Encode for Operation {
    fn encode_to<T: parity_scale_codec::Output + ?Sized>(&self, dest: &mut T) {
        self.tag().encode_to(dest);
        match self {
            Operation::AdvanceTillEthereumBlock { block_hash } => block_hash.encode_to(dest),
            Operation::ProgressTasks => {}
            Operation::ProcessQueues { gas_allowance } => gas_allowance.encode_to(dest),
            Operation::Injected(signed_tx) => signed_tx.encode_to(dest),
        }
    }
}

/// Ordered list of [`Operation`]s; CAS key = Blake2b-256 of the SCALE-encoded list.
#[derive(
    Clone, Debug, Default, PartialEq, Eq, Encode, Decode, TypeInfo, Deref, DerefMut, IntoIterator,
)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub struct Operations(pub Vec<Operation>);

impl Operations {
    pub fn new(operations: Vec<Operation>) -> Self {
        Self(operations)
    }

    /// CAS key: Blake2b-256 over the SCALE-encoded list.
    pub fn hash(&self) -> H256 {
        gear_core::utils::hash(&self.encode()).into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_txs() -> Operations {
        Operations::new(alloc::vec![
            Operation::ProgressTasks,
            Operation::ProcessQueues {
                gas_allowance: 1234,
            },
        ])
    }

    #[test]
    fn hash_is_deterministic_for_same_content() {
        let a = empty_txs();
        let b = empty_txs();
        assert_eq!(a.hash(), b.hash());
    }

    #[test]
    fn hash_changes_when_operations_change() {
        let mut a = empty_txs();
        let b = empty_txs();
        a.push(Operation::AdvanceTillEthereumBlock {
            // SAFETY: synthetic chain hash for tests — same invariant as a real EB hash.
            block_hash: unsafe { HashOf::<EB>::new(H256::from_low_u64_be(0xEB)) },
        });
        assert_ne!(a.hash(), b.hash());
    }

    #[test]
    fn operation_tag_distinguishes_variants() {
        let advance = Operation::AdvanceTillEthereumBlock {
            block_hash: HashOf::<EB>::zero(),
        };
        let progress = Operation::ProgressTasks;
        let queues = Operation::ProcessQueues {
            gas_allowance: 1234,
        };
        assert!(advance.is_advance_till_ethereum_block());
        assert!(progress.is_progress_tasks());
        assert!(queues.is_process_queues());
    }

    #[test]
    fn operation_encoding_is_frozen() {
        // The `Encode`/`Decode` impls hand-roll a fixed-width little-endian
        // `u32` tag, so the SCALE TypeInfo (derived) does NOT describe the real
        // wire format and the type-info-hash guard can't see a tag change. Pin
        // the exact leading tag bytes here: these discriminants are part of the
        // consensus wire format and must stay frozen forever.
        assert_eq!(
            Operation::AdvanceTillEthereumBlock {
                block_hash: HashOf::<EB>::zero()
            }
            .tag(),
            0
        );
        assert_eq!(Operation::ProgressTasks.tag(), 1);
        assert_eq!(Operation::ProcessQueues { gas_allowance: 0 }.tag(), 2);

        assert_eq!(
            &Operation::AdvanceTillEthereumBlock {
                block_hash: HashOf::<EB>::zero()
            }
            .encode()[..4],
            &[0, 0, 0, 0],
        );
        assert_eq!(Operation::ProgressTasks.encode(), [1, 0, 0, 0]);
        assert_eq!(
            &Operation::ProcessQueues { gas_allowance: 0 }.encode()[..4],
            &[2, 0, 0, 0],
        );

        // Unknown tag must be rejected by `Decode`, not interpreted.
        use parity_scale_codec::DecodeAll;
        assert!(Operation::decode_all(&mut [4u8, 0, 0, 0].as_slice()).is_err());
    }

    #[test]
    fn scale_round_trip_preserves_hash() {
        // `Operations` is SCALE-encoded for both the CAS payload
        // and the consensus wire payload — make sure round-trip is
        // hash-preserving so peers and the executor agree on the
        // CAS key.
        use parity_scale_codec::Decode;

        let original = Operations::new(alloc::vec![Operation::AdvanceTillEthereumBlock {
            // SAFETY: synthetic chain hash for tests — same invariant as a real EB hash.
            block_hash: unsafe { HashOf::<EB>::new(H256::from_low_u64_be(0xEB)) }
        }]);
        let encoded = original.encode();
        let decoded = Operations::decode(&mut encoded.as_slice()).expect("decode");
        assert_eq!(original, decoded);
        assert_eq!(original.hash(), decoded.hash());
    }

    #[test]
    fn block_payload_new_accepts_at_or_below_cap() {
        BlockPayload::new(alloc::vec![]).expect("empty payload");
        BlockPayload::new(alloc::vec![0u8; MAX_BLOCK_PAYLOAD_BYTES]).expect("payload at cap");
    }

    #[test]
    fn block_payload_new_rejects_above_cap() {
        let err = BlockPayload::new(alloc::vec![0u8; MAX_BLOCK_PAYLOAD_BYTES + 1])
            .expect_err("over-cap must reject");
        assert!(
            err.to_string()
                .contains(&MAX_BLOCK_PAYLOAD_BYTES.to_string()),
            "expected cap-sized error mention, got: {err}",
        );
    }

    #[test]
    fn block_payload_decode_rejects_oversized_bytes_field() {
        // Hand-roll an encoded `BlockPayload` whose `bytes` length
        // exceeds the cap. SCALE prefixes `Vec<u8>` with a `Compact<u32>`
        // length; we use the 4-byte mode for clarity. Decode must reject
        // before allocating the over-cap buffer.
        use parity_scale_codec::DecodeAll;

        let oversize = (MAX_BLOCK_PAYLOAD_BYTES + 1) as u32;
        let mut encoded = alloc::vec::Vec::new();
        encoded.extend_from_slice(&BLOCK_PAYLOAD_VERSION.encode());
        encoded.extend_from_slice(&parity_scale_codec::Compact(oversize).encode());
        encoded.extend(core::iter::repeat_n(0u8, oversize as usize));
        BlockPayload::decode_all(&mut encoded.as_slice())
            .expect_err("decode must reject over-cap payload");
    }
}
