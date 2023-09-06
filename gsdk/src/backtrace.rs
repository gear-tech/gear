// This file is part of Gear.
//
// Copyright (C) 2021-2023 Gear Technologies Inc.
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
//
//! Backtrace support for `gsdk`
use crate::TxStatus;
use indexmap::IndexMap;
use sp_core::H256;
use std::{collections::BTreeMap, time::SystemTime};

/// Transaction Status for Backtrace
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BacktraceStatus {
    Future,
    Ready,
    Broadcast(Vec<String>),
    InBlock {
        block_hash: H256,
        extrinsic_hash: H256,
    },
    Retracted {
        block_hash: H256,
    },
    FinalityTimeout {
        block_hash: H256,
    },
    Finalized {
        block_hash: H256,
        extrinsic_hash: H256,
    },
    Usurped {
        extrinsic_hash: H256,
    },
    Dropped,
    Invalid,
}

impl From<TxStatus> for BacktraceStatus {
    fn from(status: TxStatus) -> BacktraceStatus {
        match status {
            TxStatus::Future => BacktraceStatus::Future,
            TxStatus::Ready => BacktraceStatus::Ready,
            TxStatus::Broadcast(v) => BacktraceStatus::Broadcast(v),
            TxStatus::InBlock(b) => BacktraceStatus::InBlock {
                block_hash: b.block_hash(),
                extrinsic_hash: b.extrinsic_hash(),
            },
            TxStatus::Retracted(block_hash) => BacktraceStatus::Retracted { block_hash },
            TxStatus::FinalityTimeout(block_hash) => {
                BacktraceStatus::FinalityTimeout { block_hash }
            }
            TxStatus::Finalized(b) => BacktraceStatus::Finalized {
                block_hash: b.block_hash(),
                extrinsic_hash: b.extrinsic_hash(),
            },
            TxStatus::Usurped(extrinsic_hash) => BacktraceStatus::Usurped { extrinsic_hash },
            TxStatus::Dropped => BacktraceStatus::Dropped,
            TxStatus::Invalid => BacktraceStatus::Invalid,
        }
    }
}

/// Backtrace support for transactions
#[derive(Clone, Debug)]
pub struct Backtrace {
    inner: IndexMap<H256, BTreeMap<SystemTime, BacktraceStatus>>,
}
