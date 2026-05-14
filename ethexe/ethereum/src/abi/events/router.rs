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

use crate::abi::{IRouter, utils::*};
use ethexe_common::{Digest, HashOf, events::router::*};

impl From<IRouter::BatchCommitted> for BatchCommittedEvent {
    fn from(value: IRouter::BatchCommitted) -> Self {
        Self {
            digest: Digest(bytes32_to_h256(value.hash).0),
        }
    }
}

impl From<IRouter::AnnouncesCommitted> for AnnouncesCommittedEvent {
    fn from(value: IRouter::AnnouncesCommitted) -> Self {
        // # Safety because of implementation
        Self(unsafe { HashOf::new(value.head.0.into()) })
    }
}

impl From<IRouter::CodeGotValidated> for CodeGotValidatedEvent {
    fn from(value: IRouter::CodeGotValidated) -> Self {
        Self {
            code_id: bytes32_to_code_id(value.codeId),
            valid: value.valid,
        }
    }
}

impl From<IRouter::ComputationSettingsChanged> for ComputationSettingsChangedEvent {
    fn from(value: IRouter::ComputationSettingsChanged) -> Self {
        Self {
            threshold: value.threshold,
            wvara_per_second: value.wvaraPerSecond,
        }
    }
}

impl From<IRouter::ProgramCreated> for ProgramCreatedEvent {
    fn from(value: IRouter::ProgramCreated) -> Self {
        Self {
            actor_id: address_to_actor_id(value.actorId),
            code_id: bytes32_to_code_id(value.codeId),
        }
    }
}

impl From<IRouter::StorageSlotChanged> for StorageSlotChangedEvent {
    fn from(value: IRouter::StorageSlotChanged) -> Self {
        Self {
            slot: bytes32_to_h256(value.slot),
        }
    }
}

impl From<IRouter::ValidatorsCommittedForEra> for ValidatorsCommittedForEraEvent {
    fn from(value: IRouter::ValidatorsCommittedForEra) -> Self {
        Self {
            era_index: value
                .eraIndex
                .try_into()
                .expect("next era start timestamp is too large"),
        }
    }
}
