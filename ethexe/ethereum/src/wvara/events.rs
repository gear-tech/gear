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

use crate::{decode_log, IWrappedVara};
use alloy::{primitives::B256, rpc::types::eth::Log, sol_types::SolEvent};
use anyhow::{anyhow, Result};
use ethexe_common::wvara;
use signatures::*;

pub mod signatures {
    use super::*;

    crate::signatures_consts! {
        IWrappedVara;
        TRANSFER: Transfer,
        APPROVAL: Approval,
    }

    pub const FOR_HANDLING: &[B256] = &[TRANSFER];
}

pub fn try_extract_event(log: &Log) -> Result<Option<wvara::Event>> {
    let Some(topic0) = log.topic0().filter(|&v| ALL.contains(v)) else {
        return Ok(None);
    };

    let event = match *topic0 {
        TRANSFER => decode_log::<IWrappedVara::Transfer>(log)?.into(),
        APPROVAL => decode_log::<IWrappedVara::Approval>(log)?.into(),
        _ => unreachable!("filtered above"),
    };

    Ok(Some(event))
}

pub fn try_extract_event_for_handling(log: &Log) -> Result<Option<wvara::EventForHandling>> {
    if log.topic0().filter(|&v| FOR_HANDLING.contains(v)).is_none() {
        return Ok(None);
    }

    let event_for_handling = try_extract_event(log)?
        .and_then(|v| v.as_for_handling())
        .expect("filtered above");

    Ok(Some(event_for_handling))
}