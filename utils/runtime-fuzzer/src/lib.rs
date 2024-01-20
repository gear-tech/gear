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
mod gear_calls;
mod runtime;
#[cfg(test)]
mod tests;
mod utils;

pub use data::FuzzerInput;

use arbitrary::{Arbitrary, Error, Result, Unstructured};
use data::*;
use frame_support::pallet_prelude::DispatchResultWithPostInfo;
use gear_call_gen::{ClaimValueArgs, GearCall, SendMessageArgs, SendReplyArgs, UploadProgramArgs};
use gear_calls::GearCalls;
use gear_core::ids::ProgramId;
use pallet_balances::Pallet as BalancesPallet;
use runtime::*;
use sha1::*;
use std::{any, fmt::Debug, marker::PhantomData, mem};
use utils::default_generator_set;
use vara_runtime::{AccountId, Gear, Runtime, RuntimeOrigin};

/// Runs all the fuzz testing internal machinery.
pub fn run(fuzzer_input: FuzzerInput<'_>) -> Result<()> {
    todo!()
}

struct ExecutionEnvironment<'a> {
    unstructured: Unstructured<'a>,
}

impl<'a> ExecutionEnvironment<'a> {
    fn new(fulfilled_data_requirement: FulfilledDataRequirement<'a, Self>) -> Self {
        Self {
            unstructured: Unstructured::new(fulfilled_data_requirement.data),
        }
    }

    const fn random_data_requirement() -> usize {
        const VALUE_SIZE: usize = mem::size_of::<u128>();

        VALUE_SIZE
            * (GearCallsGenerator::UPLOAD_PROGRAM_CALLS + GearCallsGenerator::SEND_MESSAGE_CALLS)
            + GearCallsGenerator::AUXILIARY_SIZE
    }
}

const _: () = assert!(GearCallsGenerator::MAX_PAYLOAD_SIZE <= gear_core::message::MAX_PAYLOAD_SIZE);
const _: () = assert!(GearCallsGenerator::MAX_SALT_SIZE <= gear_core::message::MAX_PAYLOAD_SIZE);

struct GearCallsGenerator<'a> {
    unstructured: Unstructured<'a>,
}

impl<'a> GearCallsGenerator<'a> {
    // *WARNING*:
    //
    // Increasing these constants requires resetting minimal
    // size of fuzzer input buffer in corresponding scripts.
    const UPLOAD_PROGRAM_CALLS: usize = 10;
    const SEND_MESSAGE_CALLS: usize = 15;

    // Max code size - 25 KiB.
    const MAX_CODE_SIZE: usize = 25 * 1024;

    /// Maximum payload size for the fuzzer - 1 KiB.
    ///
    /// TODO: #3442
    const MAX_PAYLOAD_SIZE: usize = 1024;

    /// Maximum salt size for the fuzzer - 512 bytes.
    ///
    /// There's no need in large salts as we have only 35 extrinsics
    /// for one run. Also small salt will make overall size of the
    /// corpus smaller.
    const MAX_SALT_SIZE: usize = 512;

    const ID_SIZE: usize = mem::size_of::<ProgramId>();
    const GAS_AND_VALUE_SIZE: usize = mem::size_of::<(u64, u128)>();

    /// Used to make sure that generators will not exceed `Unstructured` size as it's used not only
    /// to generate things like wasm code or message payload but also to generate some auxiliary
    /// data, for example index in some vec.
    const AUXILIARY_SIZE: usize = 512;

    fn new(fulfilled_data_requirement: FulfilledDataRequirement<'a, Self>) -> Self {
        Self {
            unstructured: Unstructured::new(fulfilled_data_requirement.data),
        }
    }

    const fn random_data_requirement() -> usize {
        Self::upload_program_data_requirement() * Self::UPLOAD_PROGRAM_CALLS
            + Self::send_message_data_requirement() * Self::SEND_MESSAGE_CALLS
    }

    const fn upload_program_data_requirement() -> usize {
        Self::MAX_CODE_SIZE
            + Self::MAX_SALT_SIZE
            + Self::MAX_PAYLOAD_SIZE
            + Self::GAS_AND_VALUE_SIZE
            + Self::AUXILIARY_SIZE
    }

    const fn send_message_data_requirement() -> usize {
        Self::ID_SIZE + Self::MAX_PAYLOAD_SIZE + Self::GAS_AND_VALUE_SIZE + Self::AUXILIARY_SIZE
    }
}

fn run_impl_refactored(data: &[u8]) -> Result<()> {
    log::trace!(
        "New GearCalls generation: random data received {}",
        data.len()
    );
    let test_input_id = get_sha1_string(data);
    log::trace!("Generating GearCalls from corpus - {}", test_input_id);

    Ok(())
}

fn run_impl(data: &[u8]) -> Result<sp_io::TestExternalities> {
    log::trace!(
        "New GearCalls generation: random data received {}",
        data.len()
    );
    let test_input_id = get_sha1_string(data);
    log::trace!("Generating GearCalls from corpus - {}", test_input_id);

    let sender = runtime::account(runtime::alice());
    let sender_prog_id = ProgramId::from(*<AccountId as AsRef<[u8; 32]>>::as_ref(&sender));

    let generators = default_generator_set(test_input_id);
    let gear_calls = GearCalls::new(data, generators, vec![sender_prog_id])?;

    let mut test_ext = new_test_ext();
    test_ext.execute_with(|| -> Result<()> {
        // Increase maximum balance of the `sender`.
        {
            increase_to_max_balance(sender.clone())
                .unwrap_or_else(|e| unreachable!("Balance update failed: {e:?}"));
            log::info!(
                "Current balance of the sender - {}",
                BalancesPallet::<Runtime>::free_balance(&sender)
            );
        }

        for gear_call in gear_calls {
            let gear_call = gear_call?;
            let call_res = execute_gear_call(sender.clone(), gear_call);
            log::info!("Extrinsic result: {call_res:?}");
            // Run task and message queues with max possible gas limit.
            run_to_next_block();
        }

        Ok(())
    })?;

    Ok(test_ext)
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
