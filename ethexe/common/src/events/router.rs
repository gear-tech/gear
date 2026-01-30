// This file is part of Gear.
//
// Copyright (C) 2024-2025 Gear Technologies Inc.
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

use crate::{Announce, Digest, HashOf};
use gprimitives::{ActorId, CodeId, H256};
use parity_scale_codec::{Decode, Encode};

// TODO: consider to sort events in same way as in IRouter.sol

#[derive(Clone, Debug, Encode, Decode, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BatchCommittedEvent {
    pub digest: Digest,
}

#[derive(Clone, Debug, Encode, Decode, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AnnouncesCommittedEvent(pub HashOf<Announce>);

#[derive(Clone, Debug, Encode, Decode, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CodeGotValidatedEvent {
    pub code_id: CodeId,
    pub valid: bool,
}

#[derive(Clone, Debug, Encode, Decode, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CodeValidationRequestedEvent {
    pub code_id: CodeId,
    pub timestamp: u64,
    pub tx_hash: H256,
}

#[derive(Clone, Debug, Encode, Decode, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ComputationSettingsChangedEvent {
    pub threshold: u64,
    pub wvara_per_second: u128,
}

#[derive(Clone, Debug, Encode, Decode, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProgramCreatedEvent {
    pub actor_id: ActorId,
    pub code_id: CodeId,
}

#[derive(Clone, Debug, Encode, Decode, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StorageSlotChangedEvent {
    pub slot: H256,
}

#[derive(Clone, Debug, Encode, Decode, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ValidatorsCommittedForEraEvent {
    pub era_index: u64,
}

#[derive(Clone, Debug, Encode, Decode, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Event {
    BatchCommitted(BatchCommittedEvent),
    AnnouncesCommitted(AnnouncesCommittedEvent),
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
            Self::CodeValidationRequested(CodeValidationRequestedEvent {
                code_id,
                timestamp,
                tx_hash,
            }) => RequestEvent::CodeValidationRequested {
                code_id,
                timestamp,
                tx_hash,
            },
            Self::ComputationSettingsChanged(ComputationSettingsChangedEvent {
                threshold,
                wvara_per_second,
            }) => RequestEvent::ComputationSettingsChanged {
                threshold,
                wvara_per_second,
            },
            Self::ProgramCreated(ProgramCreatedEvent { actor_id, code_id }) => {
                RequestEvent::ProgramCreated { actor_id, code_id }
            }
            Self::StorageSlotChanged(StorageSlotChangedEvent { slot }) => {
                RequestEvent::StorageSlotChanged { slot }
            }
            Self::ValidatorsCommittedForEra(ValidatorsCommittedForEraEvent { era_index }) => {
                RequestEvent::ValidatorsCommittedForEra { era_index }
            }
            Self::CodeGotValidated { .. }
            | Self::AnnouncesCommitted(_)
            | Self::BatchCommitted { .. } => return None,
        })
    }
}

// TODO: consider to refactor in the same way (https://github.com/gear-tech/gear/pull/5107#discussion_r2727448994)

#[derive(Clone, Debug, Encode, Decode, PartialEq, Eq)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub enum RequestEvent {
    CodeValidationRequested {
        code_id: CodeId,
        timestamp: u64,
        // TODO (breathx): replace with `code: Vec<u8>`
        tx_hash: H256,
    },
    ComputationSettingsChanged {
        threshold: u64,
        wvara_per_second: u128,
    },
    ProgramCreated {
        actor_id: ActorId,
        code_id: CodeId,
    },
    StorageSlotChanged {
        slot: H256,
    },
    ValidatorsCommittedForEra {
        era_index: u64,
    },
}
