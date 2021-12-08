// This file is part of Gear.

// Copyright (C) 2021 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use codec::{Decode, Encode};
use core_runner::{ExecutionOutcome, RunResult};
use primitive_types::H256;
use sp_std::prelude::*;

#[derive(Debug, Encode, Decode)]
pub enum Error {
    Trap,
    Runner,
}

#[derive(Debug, Encode, Decode)]
pub struct ExecutionReport {
    pub outcome: Result<(), Vec<u8>>,
    pub wait_interrupt: bool,
    pub gas_charge: (H256, u64),
    pub messages: Vec<gear_common::Message>,
    pub log: Vec<gear_common::Message>,
    pub awakening: Vec<H256>,
}

// Todo write normal impl
impl ExecutionReport {
    pub fn from_run_result(res: RunResult) -> Self {
        let RunResult {
            outcome,
            program,
            messages,
            gas_spent,
            awakening,
        } = res;

        let wait_interrupt = outcome.wait_interrupt();

        let outcome = if let ExecutionOutcome::Trap(expl) = outcome {
            if !outcome.wait_interrupt() {
                Err(alloc::string::String::from(expl).encode())
            } else {
                Ok(())
            }
        } else {
            Ok(())
        };

        let gas_charge = (H256::from_slice(program.id().as_slice()), gas_spent);

        let mut msgs: Vec<gear_common::Message> = Vec::new();
        let mut log: Vec<gear_common::Message> = Vec::new();

        for msg in messages {
            if gear_common::program_exists(H256::from_slice(msg.dest().as_slice())) {
                msgs.push(msg.into())
            } else {
                log.push(msg.into());
            }
        }

        let messages = msgs;

        let awakening = awakening
            .iter()
            .map(|id| H256::from_slice(id.as_slice()))
            .collect();

        Self {
            outcome,
            wait_interrupt,
            gas_charge,
            messages,
            log,
            awakening,
        }
    }
}
