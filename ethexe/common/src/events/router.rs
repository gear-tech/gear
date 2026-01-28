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
use scale_info::TypeInfo;

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
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct CodeValidationRequestedEvent {
    pub code_id: CodeId,
    pub timestamp: u64,
    pub tx_hash: H256,
}

#[derive(Clone, Debug, Encode, Decode, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct ComputationSettingsChangedEvent {
    pub threshold: u64,
    pub wvara_per_second: u128,
}

#[derive(Clone, Debug, Encode, Decode, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct ProgramCreatedEvent {
    pub actor_id: ActorId,
    pub code_id: CodeId,
}

#[derive(Clone, Debug, Encode, Decode, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct StorageSlotChangedEvent {
    pub slot: H256,
}

#[derive(Clone, Debug, Encode, Decode, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct ValidatorsCommittedForEraEvent {
    pub era_index: u64,
}

#[derive(Clone, Debug, Encode, Decode, TypeInfo, PartialEq, Eq, PartialOrd, Ord, Hash)]
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
            | Self::AnnouncesCommitted(_)
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
