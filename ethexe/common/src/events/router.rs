// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use crate::Digest;
use gprimitives::{ActorId, CodeId, H256};
use parity_scale_codec::{Decode, Encode};
use scale_info::TypeInfo;

// TODO: consider to sort events in same way as in IRouter.sol

#[derive(Clone, Debug, Encode, Decode, TypeInfo, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BatchCommittedEvent {
    pub digest: Digest,
}

/// Emitted when an MB-driven chain commitment lands on-chain. The inner
/// `H256` is the MB hash that became `last_committed_mb` for the block.
#[derive(Clone, Debug, Encode, Decode, TypeInfo, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MBCommittedEvent(pub H256);

/// Carries the latest folded-in Ethereum block hash from a chain commitment.
#[derive(Clone, Debug, Encode, Decode, TypeInfo, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EBCommittedEvent(pub H256);

#[derive(Clone, Debug, Encode, Decode, TypeInfo, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CodeGotValidatedEvent {
    pub code_id: CodeId,
    pub valid: bool,
}

#[derive(Clone, Debug, Encode, Decode, TypeInfo, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct CodeValidationRequestedEvent {
    pub code_id: CodeId,
    pub timestamp: u64,
    pub tx_hash: H256,
}

#[derive(Clone, Debug, Encode, Decode, TypeInfo, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct ComputationSettingsChangedEvent {
    pub threshold: u64,
    pub wvara_per_second: u128,
}

#[derive(Clone, Debug, Encode, Decode, TypeInfo, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct ProgramCreatedEvent {
    pub actor_id: ActorId,
    pub code_id: CodeId,
}

#[derive(Clone, Debug, Encode, Decode, TypeInfo, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct StorageSlotChangedEvent {
    pub slot: H256,
}

#[derive(Clone, Debug, Encode, Decode, TypeInfo, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct ValidatorsCommittedForEraEvent {
    pub era_index: u64,
}

#[derive(Clone, Debug, Encode, Decode, TypeInfo, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Event {
    BatchCommitted(BatchCommittedEvent),
    MBCommitted(MBCommittedEvent),
    EBCommitted(EBCommittedEvent),
    CodeGotValidated(CodeGotValidatedEvent),
    CodeValidationRequested(CodeValidationRequestedEvent),
    ComputationSettingsChanged(ComputationSettingsChangedEvent),
    ProgramCreated(ProgramCreatedEvent),
    // TODO: on review ask about backward compatibility
    StorageSlotChanged(StorageSlotChangedEvent),
    ValidatorsCommittedForEra(ValidatorsCommittedForEraEvent),
}

impl Event {
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

#[derive(Clone, Debug, Encode, Decode, PartialEq, Eq)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub enum RequestEvent {
    CodeValidationRequested(CodeValidationRequestedEvent),
    ComputationSettingsChanged(ComputationSettingsChangedEvent),
    ProgramCreated(ProgramCreatedEvent),
    StorageSlotChanged(StorageSlotChangedEvent),
    ValidatorsCommittedForEra(ValidatorsCommittedForEraEvent),
}
