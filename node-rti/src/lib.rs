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

#[cfg(feature = "std")]
pub mod ext;

#[cfg(feature = "std")]
pub mod runner;

use codec::{Decode, Encode};
use sp_core::H256;
use sp_runtime_interface::runtime_interface;

#[cfg(feature = "std")]
use gear_core::{message::MessageId, program::ProgramId, storage::Storage};
#[cfg(feature = "std")]
use gear_core_runner::{
    ExecutionOutcome, ExtMessage, InitializeProgramInfo, MessageDispatch, RunNextResult,
};
#[cfg(not(feature = "std"))]
use sp_std::prelude::Vec;

#[derive(Debug, Encode, Decode)]
pub enum Error {
    Trap,
    Runner,
}

#[derive(Debug, Encode, Decode)]
pub struct ExecutionReport {
    pub handled: u32,
    pub log: Vec<gear_common::Message>,
    pub gas_refunds: Vec<(H256, u64)>,
    pub gas_charges: Vec<(H256, u64)>,
    pub gas_transfers: Vec<(H256, H256, u64)>,
    pub outcomes: Vec<(H256, Result<(), Vec<u8>>)>,
}

#[cfg(feature = "std")]
impl ExecutionReport {
    fn collect(message_queue: ext::ExtMessageQueue, result: RunNextResult) -> Self {
        let RunNextResult {
            handled,
            gas_left,
            gas_spent,
            gas_requests,
            outcomes,
            ..
        } = result;

        let log = message_queue
            .log
            .into_iter()
            .map(Into::into)
            .collect::<Vec<_>>();

        ExecutionReport {
            handled: handled as _,
            log,
            gas_refunds: gas_left
                .into_iter()
                .map(|(program_id, gas_left)| (H256::from_slice(program_id.as_slice()), gas_left))
                .collect(),
            gas_charges: gas_spent
                .into_iter()
                .map(|(program_id, gas_left)| (H256::from_slice(program_id.as_slice()), gas_left))
                .collect(),
            gas_transfers: gas_requests
                .into_iter()
                .map(|(source_id, dest_id, gas_requested)| {
                    (
                        H256::from_slice(source_id.as_slice()),
                        H256::from_slice(dest_id.as_slice()),
                        gas_requested,
                    )
                })
                .collect(),
            outcomes: outcomes
                .into_iter()
                .map(|(message_id, exec_outcome)| {
                    (
                        H256::from_slice(message_id.as_slice()),
                        match exec_outcome {
                            ExecutionOutcome::Normal => Ok(()),
                            ExecutionOutcome::Trap(t) => match t {
                                Some(s) => Err(String::from(s).encode()),
                                _ => Err(vec![]),
                            },
                        },
                    )
                })
                .collect(),
        }
    }
}

#[runtime_interface]
pub trait GearExecutor {
    fn process(max_gas_limit: u64) -> Result<ExecutionReport, Error> {
        let mut runner = crate::runner::new();

        let result = runner.run_next(max_gas_limit);

        let Storage { message_queue, .. } = runner.complete();

        Ok(ExecutionReport::collect(message_queue, result))
    }

    fn init_program(
        caller_id: H256,
        program_id: H256,
        program_code: Vec<u8>,
        init_message_id: H256,
        init_payload: Vec<u8>,
        gas_limit: u64,
        value: u128,
    ) -> Result<ExecutionReport, Error> {
        let mut runner = crate::runner::new();

        let init_message_id = MessageId::from_slice(&init_message_id[..]);
        let run_result = runner
            .init_program(InitializeProgramInfo {
                new_program_id: ProgramId::from_slice(&program_id[..]),
                source_id: ProgramId::from_slice(&caller_id[..]),
                code: program_code,
                message: ExtMessage {
                    id: init_message_id,
                    payload: init_payload,
                    gas_limit,
                    value,
                },
            })
            .map_err(|e| {
                log::error!("Error initialization program: {:?}", e);
                Error::Runner
            })?;

        let result = RunNextResult::from_single(
            init_message_id,
            ProgramId::from_slice(&caller_id[..]),
            ProgramId::from_slice(&program_id[..]),
            run_result,
        );

        let Storage { message_queue, .. } = runner.complete();

        Ok(ExecutionReport::collect(message_queue, result))
    }

    fn gas_spent(program_id: H256, payload: Vec<u8>, value: u128) -> Result<u64, Error> {
        let mut runner = crate::runner::new();

        runner.queue_message(MessageDispatch {
            source_id: ProgramId::from_slice(&H256::from_low_u64_be(1)[..]),
            destination_id: ProgramId::from_slice(&program_id[..]),
            data: ExtMessage {
                id: MessageId::from_slice(&gear_common::next_message_id(&payload)[..]),
                gas_limit: u64::MAX,
                payload: payload,
                value,
            },
        });

        let mut total_gas_spent = 0;

        loop {
            let run_result = runner.run_next(u64::MAX);

            if let Some(gas_spent) = run_result.gas_spent.first() {
                total_gas_spent += gas_spent.1;
            }

            if run_result.any_traps() {
                log::error!("gas_spent: Empty run result");
                return Err(Error::Runner);
            }

            if run_result.handled == 0 {
                break;
            }
        }

        runner.complete();

        Ok(total_gas_spent)
    }
}
