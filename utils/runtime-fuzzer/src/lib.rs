// This file is part of Gear.

// Copyright (C) 2021-2024 Gear Technologies Inc.
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

#![allow(clippy::items_after_test_module)]

mod data;
mod generator;
mod runtime;
#[cfg(test)]
mod tests;

pub use data::FuzzerInput;

use frame_support::pallet_prelude::DispatchResultWithPostInfo;
use gear_call_gen::{ClaimValueArgs, GearCall, SendMessageArgs, SendReplyArgs, UploadProgramArgs};
use gear_wasm_gen::wasm_gen_arbitrary::Result;
use generator::*;
use runtime::BalanceManager;
use sha1::Digest;
use vara_runtime::{AccountId, Gear, RuntimeOrigin};

pub fn run(fuzzer_input: FuzzerInput<'_>) -> Result<()> {
    run_impl(fuzzer_input).map(|_| ())
}

/// Runs all the fuzz testing internal machinery.
fn run_impl(fuzzer_input: FuzzerInput<'_>) -> Result<sp_io::TestExternalities> {
    let raw_data = fuzzer_input.inner();
    let (balance_manager_data_requirement, generator_data_requirement) =
        fuzzer_input.into_data_requirements()?;

    log::trace!(
        "New gear calls generation: random data received {}",
        raw_data.len()
    );
    let corpus_id = get_sha1_string(raw_data);
    log::trace!("Generating gear calls from corpus - {}", corpus_id);

    let mut balance_manager =
        BalanceManager::new(runtime::alice(), balance_manager_data_requirement);
    let mut test_ext = runtime::new_test_ext();
    let mut env_producer = RuntimeStateViewProducer::new(corpus_id, balance_manager.sender.clone());
    let mut generator = GearCallsGenerator::new(generator_data_requirement);
    loop {
        let must_stop = test_ext.execute_with(|| -> Result<bool> {
            let env = env_producer.produce_state_view(balance_manager.update_balance()?);
            let Some(gear_call) = generator.generate(env)? else {
                return Ok(true);
            };

            let call_res = execute_gear_call(balance_manager.sender.clone(), gear_call);
            log::info!("Extrinsic result: {call_res:?}");

            // Run task and message queues with max possible gas limit.
            runtime::run_to_next_block();

            Ok(false)
        })?;

        if must_stop {
            break Ok(test_ext);
        }
    }
}

fn execute_gear_call(sender: AccountId, call: GearCall) -> DispatchResultWithPostInfo {
    match call {
        GearCall::UploadProgram(args) => {
            let UploadProgramArgs((code, salt, payload, gas_limit, value)) = args;
            Gear::upload_program(
                RuntimeOrigin::signed(sender),
                code,
                salt,
                payload,
                gas_limit,
                value,
                false,
            )
        }
        GearCall::SendMessage(args) => {
            let SendMessageArgs((destination, payload, gas_limit, value)) = args;
            Gear::send_message(
                RuntimeOrigin::signed(sender),
                destination,
                payload,
                gas_limit,
                value,
                false,
            )
        }
        GearCall::SendReply(args) => {
            let SendReplyArgs((message_id, payload, gas_limit, value)) = args;
            Gear::send_reply(
                RuntimeOrigin::signed(sender),
                message_id,
                payload,
                gas_limit,
                value,
                false,
            )
        }
        GearCall::ClaimValue(args) => {
            let ClaimValueArgs(message_id) = args;
            Gear::claim_value(RuntimeOrigin::signed(sender), message_id)
        }
        _ => unimplemented!("Unsupported currently."),
    }
}

fn get_sha1_string(input: &[u8]) -> String {
    let mut hasher = sha1::Sha1::new();
    hasher.update(input);

    hex::encode(hasher.finalize())
}
