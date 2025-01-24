// This file is part of Gear.
//
// Copyright (C) 2024 Gear Technologies Inc.
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

use gprimitives::{ActorId, CodeId, H256};
use parity_scale_codec::{Decode, Encode};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Encode, Decode, PartialEq, Eq, PartialOrd, Ord)]
pub enum Event {
    BlockCommitted {
        hash: H256,
    },
    CodeGotValidated {
        code_id: CodeId,
        valid: bool,
    },
    CodeValidationRequested {
        code_id: CodeId,
        /// This field is replaced with tx hash in case of zero.
        blob_tx_hash: H256,
    },
    ComputationSettingsChanged {
        threshold: u64,
        wvara_per_second: u128,
    },
    ProgramCreated {
        actor_id: ActorId,
        code_id: CodeId,
    },
    StorageSlotChanged,
    NextEraValidatorsCommitted {
        next_era_start: u64,
    },
}

impl Event {
    pub fn as_request(self) -> Option<RequestEvent> {
        Some(match self {
            Self::CodeValidationRequested {
                code_id,
                blob_tx_hash,
            } => RequestEvent::CodeValidationRequested {
                code_id,
                blob_tx_hash,
            },
            Self::ComputationSettingsChanged {
                threshold,
                wvara_per_second,
            } => RequestEvent::ComputationSettingsChanged {
                threshold,
                wvara_per_second,
            },
            Self::ProgramCreated { actor_id, code_id } => {
                RequestEvent::ProgramCreated { actor_id, code_id }
            }
            Self::StorageSlotChanged => RequestEvent::StorageSlotChanged,
            Self::NextEraValidatorsCommitted { next_era_start } => {
                RequestEvent::NextEraValidatorsCommitted { next_era_start }
            }
            Self::BlockCommitted { .. } | Self::CodeGotValidated { .. } => return None,
        })
    }
}

#[derive(Clone, Debug, Encode, Decode, PartialEq, Eq, Serialize, Deserialize)]
pub enum RequestEvent {
    CodeValidationRequested {
        code_id: CodeId,
        // TODO (breathx): replace with `code: Vec<u8>`
        /// This field is replaced with tx hash in case of zero.
        blob_tx_hash: H256,
    },
    ComputationSettingsChanged {
        threshold: u64,
        wvara_per_second: u128,
    },
    ProgramCreated {
        actor_id: ActorId,
        code_id: CodeId,
    },
    StorageSlotChanged,
    NextEraValidatorsCommitted {
        next_era_start: u64,
    },
}
