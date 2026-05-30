// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use crate::Digest;
use gprimitives::{ActorId, CodeId, H256};
use parity_scale_codec::{Decode, Encode};
use scale_info::TypeInfo;

// TODO: consider to sort events in same way as in IRouter.sol

/// Emitted when all commitments in a batch have been applied on-chain.
///
/// Carries the Keccak256 `digest` of the committed batch, matching
/// the `Gear.batchCommitmentHash(...)` value from the Router contract.
#[derive(Clone, Debug, Encode, Decode, TypeInfo, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BatchCommittedEvent {
    /// Keccak256 digest of the committed batch.
    pub digest: Digest,
}

/// Emitted when an MB-driven chain commitment lands on-chain. The inner
/// `H256` is the MB hash that became `last_committed_mb` for the block.
#[derive(Clone, Debug, Encode, Decode, TypeInfo, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MBCommittedEvent(pub H256);

/// Carries the latest folded-in Ethereum block hash from a chain commitment.
#[derive(Clone, Debug, Encode, Decode, TypeInfo, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EBCommittedEvent(pub H256);

/// Emitted when a previously requested code validation completes and its `CodeState` changes.
#[derive(Clone, Debug, Encode, Decode, TypeInfo, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CodeGotValidatedEvent {
    /// Blake2b hash of the WASM blob that was validated.
    pub code_id: CodeId,
    /// `true` if the code is valid and may be used for program creation.
    pub valid: bool,
}

/// Requesting event signalling that validators must download and validate a WASM code blob
/// referenced by the given transaction.
#[derive(Clone, Debug, Encode, Decode, TypeInfo, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct CodeValidationRequestedEvent {
    /// Expected blake2b hash of the WASM blob.
    pub code_id: CodeId,
    /// Block timestamp at which the request was submitted.
    pub timestamp: u64,
    /// Ethereum transaction hash containing the blob to validate.
    pub tx_hash: H256,
}

/// Emitted when an on-chain authority updates the Router's computation settings.
///
/// Validators must apply the new settings starting from the next block.
#[derive(Clone, Debug, Encode, Decode, TypeInfo, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct ComputationSettingsChangedEvent {
    /// Gas threshold per computation unit; the default mirrors [`COMPUTATION_THRESHOLD`](crate::gear::COMPUTATION_THRESHOLD) (2_500_000_000) from the Router contract.
    pub threshold: u64,
    /// Amount of WVara charged from a program's execution balance per second of computation.
    pub wvara_per_second: u128,
}

/// Emitted when a new program is created in the co-processor and its Ethereum Mirror deployed.
///
/// Validators must initialize the program with a zeroed state hash internally.
#[derive(Clone, Debug, Encode, Decode, TypeInfo, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct ProgramCreatedEvent {
    /// Identifier of the newly created program.
    pub actor_id: ActorId,
    /// Blake2b hash of the WASM code used to create the program.
    pub code_id: CodeId,
}

/// Emitted when the Router's storage slot is wiped, invalidating all previously existing codes
/// and programs. Validators must wipe their databases immediately upon receiving this event.
#[derive(Clone, Debug, Encode, Decode, TypeInfo, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct StorageSlotChangedEvent {
    /// The new storage slot value after the wipe.
    pub slot: H256,
}

/// Emitted when the validator set for an upcoming era has been committed on-chain.
///
/// Serves as both an informational and requesting event: validators must apply the new
/// set internally at the start of the indicated era.
#[derive(Clone, Debug, Encode, Decode, TypeInfo, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct ValidatorsCommittedForEraEvent {
    /// Zero-based index of the era for which the validator set was committed.
    pub era_index: u64,
}

/// All events that can be emitted by the Router contract and observed by the off-chain layer.
#[derive(Clone, Debug, Encode, Decode, TypeInfo, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Event {
    /// A batch of state transitions was committed on-chain.
    BatchCommitted(BatchCommittedEvent),
    /// The announces chain head was committed; updates `last_committed_mb`.
    MBCommitted(MBCommittedEvent),
    /// Latest folded-in Ethereum block hash was committed; updates `last_committed_eb`.
    EBCommitted(EBCommittedEvent),
    /// A code validation request was resolved with a pass/fail result.
    CodeGotValidated(CodeGotValidatedEvent),
    /// A new code validation request was submitted by a user.
    CodeValidationRequested(CodeValidationRequestedEvent),
    /// The Router's computation settings (gas threshold and WVara rate) were updated.
    ComputationSettingsChanged(ComputationSettingsChangedEvent),
    /// A new program and its Mirror contract were created.
    ProgramCreated(ProgramCreatedEvent),
    // TODO: on review ask about backward compatibility
    /// The Router's storage slot changed, invalidating all prior state.
    StorageSlotChanged(StorageSlotChangedEvent),
    /// The validator set for an upcoming era was committed.
    ValidatorsCommittedForEra(ValidatorsCommittedForEraEvent),
}

impl Event {
    /// Converts this event into a [`RequestEvent`] if it carries an actionable validator request.
    ///
    /// Returns `None` for purely informational events (`BatchCommitted`, `MBCommitted`,
    /// `EBCommitted`, `CodeGotValidated`) that require no validator action.
    pub fn to_request(self) -> Option<RequestEvent> {
        Some(match self {
            Self::CodeValidationRequested(event) => RequestEvent::CodeValidationRequested(event),
            Self::ComputationSettingsChanged(event) => {
                RequestEvent::ComputationSettingsChanged(event)
            }
            Self::ProgramCreated(event) => RequestEvent::ProgramCreated(event),
            Self::StorageSlotChanged(event) => RequestEvent::StorageSlotChanged(event),
            Self::ValidatorsCommittedForEra(event) => {
                RequestEvent::ValidatorsCommittedForEra(event)
            }
            Self::CodeGotValidated { .. }
            | Self::MBCommitted(_)
            | Self::EBCommitted(_)
            | Self::BatchCommitted { .. } => return None,
        })
    }
}

/// Subset of [`Event`] variants that require an active validator response or state change.
///
/// Purely informational events (batch/MB/EB committed, code got validated) are excluded.
#[derive(Clone, Debug, Encode, Decode, PartialEq, Eq)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub enum RequestEvent {
    /// Validators must download and validate the referenced WASM blob.
    CodeValidationRequested(CodeValidationRequestedEvent),
    /// Validators must apply updated gas threshold and WVara rate from the next block.
    ComputationSettingsChanged(ComputationSettingsChangedEvent),
    /// Validators must initialize a new program with a zeroed state hash.
    ProgramCreated(ProgramCreatedEvent),
    /// Validators must wipe their databases due to a storage slot reset.
    StorageSlotChanged(StorageSlotChangedEvent),
    /// Validators must register the new validator set for the given era.
    ValidatorsCommittedForEra(ValidatorsCommittedForEraEvent),
}
