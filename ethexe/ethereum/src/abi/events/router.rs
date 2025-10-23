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
use ethexe_common::{Digest, HashOf, events::RouterEvent};

impl From<IRouter::BatchCommitted> for RouterEvent {
    fn from(value: IRouter::BatchCommitted) -> Self {
        Self::BatchCommitted {
            digest: Digest(bytes32_to_h256(value.hash).0),
        }
    }
}

impl From<IRouter::AnnouncesCommitted> for RouterEvent {
    fn from(value: IRouter::AnnouncesCommitted) -> Self {
        Self::AnnouncesCommitted(unsafe { HashOf::new(value.head.0.into()) })
    }
}

impl From<IRouter::CodeGotValidated> for RouterEvent {
    fn from(value: IRouter::CodeGotValidated) -> Self {
        Self::CodeGotValidated {
            code_id: bytes32_to_code_id(value.codeId),
            valid: value.valid,
        }
    }
}

impl From<IRouter::ComputationSettingsChanged> for RouterEvent {
    fn from(value: IRouter::ComputationSettingsChanged) -> Self {
        Self::ComputationSettingsChanged {
            threshold: value.threshold,
            wvara_per_second: value.wvaraPerSecond,
        }
    }
}

impl From<IRouter::ProgramCreated> for RouterEvent {
    fn from(value: IRouter::ProgramCreated) -> Self {
        Self::ProgramCreated {
            actor_id: address_to_actor_id(value.actorId),
            code_id: bytes32_to_code_id(value.codeId),
        }
    }
}

impl From<IRouter::StorageSlotChanged> for RouterEvent {
    fn from(_value: IRouter::StorageSlotChanged) -> Self {
        Self::StorageSlotChanged
    }
}

impl From<IRouter::NextEraValidatorsCommitted> for RouterEvent {
    fn from(value: IRouter::NextEraValidatorsCommitted) -> Self {
        Self::NextEraValidatorsCommitted {
            next_era_start: value
                .startTimestamp
                .try_into()
                .expect("next era start timestamp is too large"),
        }
    }
}
