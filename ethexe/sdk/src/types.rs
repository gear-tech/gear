// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use ethexe_common::injected::Promise;
use gprimitives::{H256, MessageId};

pub use ethexe_common::gear::ValueClaim;
pub use ethexe_ethereum::router::CodeValidationResult;

#[derive(Debug, Clone)]
pub struct InjectedMessageResult {
    pub message_id: MessageId,
    pub tx_hash: H256,
    pub reference_block_number: u32,
    pub reference_block_hash: H256,
    pub promise: Option<Promise>,
}
