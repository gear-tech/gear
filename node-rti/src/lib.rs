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
use gear_core::{program::ProgramId, storage::Storage};
#[cfg(feature = "std")]
use gear_core_runner::runner::RunNextResult;
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
}

#[cfg(feature = "std")]
impl ExecutionReport {
    fn collect(message_queue: ext::ExtMessageQueue, result: RunNextResult) -> Self {
        // TODO: actually compare touched from run result with
        //       that is what should be predefined in message
        let RunNextResult {
            handled,
            gas_left,
            gas_spent,
            gas_requests,
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
        }
    }
}

#[runtime_interface]
pub trait GearExecutor {
    fn process() -> Result<ExecutionReport, Error> {
        let mut runner = crate::runner::new();

        let result = runner.run_next().map_err(|e| {
            log::error!("Error handling message: {:?}", e);
            Error::Runner
        })?;

        let (Storage { message_queue, .. }, persistent_memory) = runner.complete();
        crate::runner::set_memory(persistent_memory);

        Ok(ExecutionReport::collect(message_queue, result))
    }

    fn init_program(
        caller_id: H256,
        program_id: H256,
        program_code: Vec<u8>,
        init_payload: Vec<u8>,
        gas_limit: u64,
        value: u128,
    ) -> Result<ExecutionReport, Error> {
        let mut runner = crate::runner::new();

        let result = RunNextResult::from_single(
            ProgramId::from_slice(&caller_id[..]),
            ProgramId::from_slice(&program_id[..]),
            runner
                .init_program(
                    ProgramId::from_slice(&caller_id[..]),
                    gear_common::caller_nonce_fetch_inc(caller_id),
                    ProgramId::from_slice(&program_id[..]),
                    program_code,
                    init_payload,
                    gas_limit,
                    value,
                )
                .map_err(|e| {
                    log::error!("Error initialization program: {:?}", e);
                    Error::Runner
                })?,
        );

        let (Storage { message_queue, .. }, persistent_memory) = runner.complete();

        crate::runner::set_memory(persistent_memory);

        Ok(ExecutionReport::collect(message_queue, result))
    }
}
