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

use crate::{decode_log, IRouter};
use alloy::{primitives::B256, rpc::types::eth::Log, sol_types::SolEvent};
use anyhow::{anyhow, Result};
use ethexe_common::router;
use signatures::*;

pub mod signatures {
    use super::*;

    crate::signatures_consts! {
        IRouter;
        BASE_WEIGHT_CHANGED: BaseWeightChanged,
        BLOCK_COMMITTED: BlockCommitted,
        CODE_GOT_VALIDATED: CodeGotValidated,
        CODE_VALIDATION_REQUESTED: CodeValidationRequested,
        PROGRAM_CREATED: ProgramCreated,
        STORAGE_SLOT_CHANGED: StorageSlotChanged,
        VALIDATORS_SET_CHANGED: ValidatorsSetChanged,
        VALUE_PER_WEIGHT_CHANGED: ValuePerWeightChanged,
    }

    pub const FOR_HANDLING: &[B256] = &[
        BASE_WEIGHT_CHANGED,
        CODE_VALIDATION_REQUESTED,
        PROGRAM_CREATED,
        STORAGE_SLOT_CHANGED,
        VALIDATORS_SET_CHANGED,
        VALUE_PER_WEIGHT_CHANGED,
    ];
}

pub fn try_extract_event(log: &Log) -> Result<Option<router::Event>> {
    let Some(topic0) = log.topic0().filter(|&v| ALL.contains(v)) else {
        return Ok(None);
    };

    let event = match *topic0 {
        BASE_WEIGHT_CHANGED => decode_log::<IRouter::BaseWeightChanged>(log)?.into(),
        BLOCK_COMMITTED => decode_log::<IRouter::BlockCommitted>(log)?.into(),
        CODE_GOT_VALIDATED => decode_log::<IRouter::CodeGotValidated>(log)?.into(),
        CODE_VALIDATION_REQUESTED => {
            let tx_hash = log
                .transaction_hash
                .ok_or_else(|| anyhow!("Tx hash not found"))?;

            let mut event = decode_log::<IRouter::CodeValidationRequested>(log)?;

            if event.blobTxHash.is_zero() {
                event.blobTxHash = tx_hash;
            }

            event.into()
        }
        PROGRAM_CREATED => decode_log::<IRouter::ProgramCreated>(log)?.into(),
        STORAGE_SLOT_CHANGED => decode_log::<IRouter::StorageSlotChanged>(log)?.into(),
        VALIDATORS_SET_CHANGED => decode_log::<IRouter::ValidatorsSetChanged>(log)?.into(),
        VALUE_PER_WEIGHT_CHANGED => decode_log::<IRouter::ValuePerWeightChanged>(log)?.into(),
        _ => unreachable!("filtered above"),
    };

    Ok(Some(event))
}

pub fn try_extract_event_for_handling(log: &Log) -> Result<Option<router::EventForHandling>> {
    if log.topic0().filter(|&v| FOR_HANDLING.contains(v)).is_none() {
        return Ok(None);
    }

    let event_for_handling = try_extract_event(log)?
        .and_then(|v| v.as_for_handling())
        .expect("filtered above");

    Ok(Some(event_for_handling))
}
