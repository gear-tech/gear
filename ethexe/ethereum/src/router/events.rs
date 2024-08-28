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

use crate::IRouter;
use alloy::{rpc::types::eth::Log, sol_types::SolEvent};
use anyhow::{anyhow, Result};
use ethexe_common::router;
use gprimitives::H256;

pub mod signatures {
    use super::{IRouter, SolEvent, H256};

    pub const BASE_WEIGHT_CHANGED: H256 = H256(IRouter::BaseWeightChanged::SIGNATURE_HASH.0);
    pub const BLOCK_COMMITTED: H256 = H256(IRouter::BlockCommitted::SIGNATURE_HASH.0);
    pub const CODE_GOT_VALIDATED: H256 = H256(IRouter::CodeGotValidated::SIGNATURE_HASH.0);
    pub const CODE_VALIDATION_REQUESTED: H256 =
        H256(IRouter::CodeValidationRequested::SIGNATURE_HASH.0);
    pub const PROGRAM_CREATED: H256 = H256(IRouter::ProgramCreated::SIGNATURE_HASH.0);
    pub const STORAGE_SLOT_CHANGED: H256 = H256(IRouter::StorageSlotChanged::SIGNATURE_HASH.0);
    pub const VALIDATORS_SET_CHANGED: H256 = H256(IRouter::ValidatorsSetChanged::SIGNATURE_HASH.0);
    pub const VALUE_PER_WEIGHT_CHANGED: H256 =
        H256(IRouter::ValuePerWeightChanged::SIGNATURE_HASH.0);

    pub const ALL: [H256; 8] = [
        BASE_WEIGHT_CHANGED,
        BLOCK_COMMITTED,
        CODE_GOT_VALIDATED,
        CODE_VALIDATION_REQUESTED,
        PROGRAM_CREATED,
        STORAGE_SLOT_CHANGED,
        VALIDATORS_SET_CHANGED,
        VALUE_PER_WEIGHT_CHANGED,
    ];
}

pub fn try_extract_event(log: &Log) -> Result<Option<router::Event>> {
    use crate::decode_log;
    use signatures::*;

    let Some(topic0) = log.topic0().map(|v| H256(v.0)) else {
        return Ok(None);
    };

    // TODO (breathx): pattern matching issue for primitive_types::H256... ????
    let event = match topic0 {
        b if b == BASE_WEIGHT_CHANGED => decode_log::<IRouter::BaseWeightChanged>(log)?.into(),
        b if b == BLOCK_COMMITTED => decode_log::<IRouter::BlockCommitted>(log)?.into(),
        b if b == CODE_GOT_VALIDATED => decode_log::<IRouter::CodeGotValidated>(log)?.into(),
        b if b == CODE_VALIDATION_REQUESTED => {
            let tx_hash = log
                .transaction_hash
                .ok_or_else(|| anyhow!("Tx hash not found"))?;

            let mut event = decode_log::<IRouter::CodeValidationRequested>(log)?;

            if event.blobTxHash.is_zero() {
                event.blobTxHash = tx_hash;
            }

            event.into()
        }
        b if b == PROGRAM_CREATED => decode_log::<IRouter::ProgramCreated>(log)?.into(),
        b if b == STORAGE_SLOT_CHANGED => decode_log::<IRouter::StorageSlotChanged>(log)?.into(),
        b if b == VALIDATORS_SET_CHANGED => {
            decode_log::<IRouter::ValidatorsSetChanged>(log)?.into()
        }
        b if b == VALUE_PER_WEIGHT_CHANGED => {
            decode_log::<IRouter::ValuePerWeightChanged>(log)?.into()
        }
        _ => return Ok(None),
    };

    Ok(Some(event))
}
