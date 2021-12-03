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

#![cfg_attr(not(feature = "std"), no_std)]
extern crate alloc;
pub mod ext;

use alloc::collections::VecDeque;
use codec::{Decode, Encode};
use sp_core::H256;

use gear_core::{
    message::{Message, MessageId},
    program::ProgramId,
    storage::{ProgramStorage, Storage},
};

use gear_backend_common::Environment;
pub use gear_core_runner::{BlockInfo, Ext};
use gear_core_runner::{
    ExecutionOutcome, ExtMessage, InitializeProgramInfo, RunNextResult, Runner,
};
use sp_std::prelude::*;

use crate::ext::*;

type ExtRunner<E> = Runner<ExtStorage, E>;
/// Storage used for running node
pub type ExtStorage = Storage<ExtProgramStorage>;

#[derive(Debug, Encode, Decode)]
pub enum Error {
    Trap,
    Runner,
}

#[derive(Debug, Encode, Decode, Default)]
pub struct ExecutionReport {
    pub messages: Vec<gear_common::Message>,
    pub program_id: H256,
    pub log: Vec<gear_common::Message>,
    pub gas_charges: Vec<(H256, u64)>,
    pub outcomes: Vec<(H256, Result<(), Vec<u8>>)>,
    pub wait_list: Vec<gear_common::Message>,
    pub awakening: Vec<H256>,
}

impl From<RunNextResult> for ExecutionReport {
    fn from(result: RunNextResult) -> Self {
        let RunNextResult {
            messages,
            prog_id,
            log,
            gas_spent,
            outcomes,
            wait_list,
            awakening,
        } = result;

        let messages = messages.into_iter().map(Into::into).collect();
        let log = log.into_iter().map(Into::into).collect();
        let wait_list = wait_list.into_iter().map(Into::into).collect();
        let awakening = awakening
            .into_iter()
            .map(|msg_id| H256::from_slice(msg_id.as_slice()))
            .collect();

        ExecutionReport {
            messages,
            program_id: H256::from_slice(prog_id.as_slice()),
            log,
            gas_charges: gas_spent
                .into_iter()
                .map(|(program_id, gas_left)| (H256::from_slice(program_id.as_slice()), gas_left))
                .collect(),
            outcomes: outcomes
                .into_iter()
                .map(|(message_id, exec_outcome)| {
                    (
                        H256::from_slice(message_id.as_slice()),
                        match exec_outcome {
                            ExecutionOutcome::Normal | ExecutionOutcome::Waiting => Ok(()),
                            ExecutionOutcome::Trap(t) => match t {
                                Some(s) => Err(alloc::string::String::from(s).encode()),
                                _ => Err(Vec::new()),
                            },
                        },
                    )
                })
                .collect(),
            wait_list,
            awakening,
        }
    }
}

pub fn process<E: Environment<Ext>>(
    message: gear_common::Message,
    block_info: BlockInfo,
) -> Result<ExecutionReport, Error> {
    // TODO: it's not required to process possible errors since
    // builder doesn't contain any program here. Check `program` method
    // for details
    let (mut runner, _) = ExtRunner::<E>::builder().block_info(block_info).build();

    Ok(runner.run_next(message.into()).into())
}

#[allow(clippy::too_many_arguments)]
pub fn init_program<E: Environment<Ext>>(
    caller_id: H256,
    program_id: H256,
    program_code: Vec<u8>,
    init_message_id: H256,
    init_payload: Vec<u8>,
    gas_limit: u64,
    value: u128,
    block_info: BlockInfo,
) -> Result<ExecutionReport, Error> {
    // TODO: it's not required to process possible errors since
    // builder doesn't contain any program here. Check `program` method
    // for details
    let (mut runner, _) = ExtRunner::<E>::builder().block_info(block_info).build();

    let init_message_id = MessageId::from_slice(&init_message_id[..]);
    let program_id = ProgramId::from_slice(&program_id[..]);
    let source_id = ProgramId::from_slice(&caller_id[..]);
    let run_result = runner
        .init_program(InitializeProgramInfo {
            new_program_id: program_id,
            source_id,
            code: program_code,
            message: ExtMessage {
                id: init_message_id,
                payload: init_payload.clone(),
                gas_limit,
                value,
            },
        })
        .map_err(|e| {
            log::error!("Error initialization program: {:?}", e);
            Error::Runner
        })?;

    let init_message = Message {
        id: init_message_id,
        source: source_id,
        dest: program_id,
        payload: init_payload.into(),
        gas_limit,
        value,
        reply: None,
    };
    let mut result = RunNextResult::from_single(init_message, run_result.clone());

    for message in run_result.messages {
        let m = message.into_message(program_id);
        if runner.storage().program_storage.exists(m.dest()) {
            result.messages.push(m);
        } else {
            result.log.push(m);
        }
    }

    if let Some(m) = run_result.reply {
        let m = m.into_message(init_message_id, program_id, source_id);
        if runner.storage().program_storage.exists(m.dest()) {
            result.messages.push(m);
        } else {
            result.log.push(m);
        }
    }

    Ok(result.into())
}

pub fn gas_spent<E: Environment<Ext>>(
    program_id: H256,
    payload: Vec<u8>,
    value: u128,
) -> Result<u64, Error> {
    let mut runner = ExtRunner::<E>::default();

    let message = Message {
        id: MessageId::from_slice(&gear_common::next_message_id(&payload)[..]),
        source: ProgramId::from(1),
        dest: ProgramId::from_slice(&program_id[..]),
        gas_limit: u64::MAX,
        payload: payload.into(),
        value,
        reply: None,
    };
    let mut messages = VecDeque::from([message]);

    let mut total_gas_spent = 0;

    while let Some(message) = messages.pop_front() {
        let mut run_result = runner.run_next(message);
        for new_message in run_result.messages.drain(..) {
            messages.push_back(new_message);
        }

        if let Some(gas_spent) = run_result.gas_spent.first() {
            total_gas_spent += gas_spent.1;
        }

        if run_result.any_traps() {
            log::error!("gas_spent: Empty run result");
            return Err(Error::Runner);
        }
    }

    runner.complete();

    Ok(total_gas_spent)
}
