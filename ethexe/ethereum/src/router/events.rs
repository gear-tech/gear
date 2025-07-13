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

use crate::{
    abi::utils::{bytes32_to_code_id, bytes32_to_h256},
    decode_log, IRouter,
};
use alloy::{primitives::B256, rpc::types::eth::Log, sol_types::SolEvent};
use anyhow::{anyhow, Result};
use ethexe_common::events::{RouterEvent, RouterRequestEvent};
use gprimitives::H256;
use signatures::*;

pub mod signatures {
    use super::*;

    crate::signatures_consts! {
        IRouter;
        BLOCK_COMMITTED: BlockCommitted,
        CODE_GOT_VALIDATED: CodeGotValidated,
        CODE_VALIDATION_REQUESTED: CodeValidationRequested,
        COMPUTATION_SETTINGS_CHANGED: ComputationSettingsChanged,
        PROGRAM_CREATED: ProgramCreated,
        STORAGE_SLOT_CHANGED: StorageSlotChanged,
        NEXT_ERA_VALIDATORS_COMMITTED: NextEraValidatorsCommitted,
        REWARDS_DISTRIBUTED: RewardsDistributed,
    }

    pub const REQUESTS: &[B256] = &[
        CODE_VALIDATION_REQUESTED,
        COMPUTATION_SETTINGS_CHANGED,
        PROGRAM_CREATED,
        STORAGE_SLOT_CHANGED,
        NEXT_ERA_VALIDATORS_COMMITTED,
    ];
}

pub fn try_extract_event(log: &Log) -> Result<Option<RouterEvent>> {
    let Some(topic0) = log.topic0().filter(|&v| ALL.contains(v)) else {
        return Ok(None);
    };

    let event = match *topic0 {
        BLOCK_COMMITTED => decode_log::<IRouter::BlockCommitted>(log)?.into(),
        CODE_GOT_VALIDATED => decode_log::<IRouter::CodeGotValidated>(log)?.into(),
        CODE_VALIDATION_REQUESTED => {
            let tx_hash = log
                .transaction_hash
                .ok_or_else(|| anyhow!("Tx hash not found"))?;
            let block_timestamp = log
                .block_timestamp
                .ok_or_else(|| anyhow!("Block timestamp not found"))?;
            let event = decode_log::<IRouter::CodeValidationRequested>(log)?;

            RouterEvent::CodeValidationRequested {
                code_id: bytes32_to_code_id(event.codeId),
                timestamp: block_timestamp,
                tx_hash: bytes32_to_h256(tx_hash),
            }
        }
        COMPUTATION_SETTINGS_CHANGED => {
            decode_log::<IRouter::ComputationSettingsChanged>(log)?.into()
        }
        PROGRAM_CREATED => decode_log::<IRouter::ProgramCreated>(log)?.into(),
        STORAGE_SLOT_CHANGED => decode_log::<IRouter::StorageSlotChanged>(log)?.into(),
        NEXT_ERA_VALIDATORS_COMMITTED => {
            decode_log::<IRouter::NextEraValidatorsCommitted>(log)?.into()
        }
        _ => unreachable!("filtered above"),
    };

    Ok(Some(event))
}

pub fn try_extract_request_event(log: &Log) -> Result<Option<RouterRequestEvent>> {
    if log.topic0().filter(|&v| REQUESTS.contains(v)).is_none() {
        return Ok(None);
    }

    let request_event = try_extract_event(log)?
        .and_then(|v| v.to_request())
        .expect("filtered above");

    Ok(Some(request_event))
}

pub fn try_extract_committed_block_hash(log: &Log) -> Result<Option<H256>> {
    if log.topic0() != Some(&BLOCK_COMMITTED) {
        return Ok(None);
    }

    decode_log::<IRouter::BlockCommitted>(log).map(|e| Some(bytes32_to_h256(e.hash)))
}
