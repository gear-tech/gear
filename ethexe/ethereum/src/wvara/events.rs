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

#![allow(unused)]

use crate::IWrappedVara;
use alloy::{rpc::types::eth::Log, sol_types::SolEvent};
use anyhow::{anyhow, Result};
use ethexe_common::wvara;
use gprimitives::H256;

pub mod signatures {
    use super::{IWrappedVara, SolEvent, H256};

    pub const TRANSFER: H256 = H256(IWrappedVara::Transfer::SIGNATURE_HASH.0);
    pub const APPROVAL: H256 = H256(IWrappedVara::Approval::SIGNATURE_HASH.0);

    pub const ALL: [H256; 2] = [TRANSFER, APPROVAL];
}

pub fn try_extract_event(log: &Log) -> Result<Option<wvara::Event>> {
    use crate::decode_log;
    use signatures::*;

    let Some(topic0) = log.topic0().map(|v| H256(v.0)) else {
        return Ok(None);
    };

    // TODO (breathx): pattern matching issue for primitive_types::H256... ????
    let event = match topic0 {
        b if b == TRANSFER => decode_log::<IWrappedVara::Transfer>(log)?.into(),
        b if b == APPROVAL => decode_log::<IWrappedVara::Approval>(log)?.into(),
        _ => return Ok(None),
    };

    Ok(Some(event))
}
