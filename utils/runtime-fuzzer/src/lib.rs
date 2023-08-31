// This file is part of Gear.

// Copyright (C) 2021-2023 Gear Technologies Inc.
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

mod arbitrary_call;
mod runtime;
#[cfg(test)]
mod tests;

pub use arbitrary_call::GearCalls;

use arbitrary_call::FuzzerRuntimeData;
use frame_support::pallet_prelude::DispatchResultWithPostInfo;
use gear_call_gen::{ClaimValueArgs, GearCall, SendMessageArgs, SendReplyArgs, UploadProgramArgs};
use gear_common::storage::{IterableByKeyMap, Messenger};
use gear_core::message::UserStoredMessage;
use gear_runtime::{AccountId, Gear, Runtime, RuntimeOrigin};
use itertools::Itertools;
use pallet_balances::Pallet as BalancesPallet;
use pallet_gear::Config;
use runtime::*;

trait RuntimeDataUpdate {
    fn apply_runtime_data_update(&self, sender: AccountId, runtime_data: &mut FuzzerRuntimeData);
}

impl RuntimeDataUpdate for GearCall {
    fn apply_runtime_data_update(&self, sender: AccountId, runtime_data: &mut FuzzerRuntimeData) {
        match self {
            GearCall::UploadProgram(..) | GearCall::SendMessage(..) => {
                let messages: Vec<UserStoredMessage> =
                    <<Runtime as Config>::Messenger as Messenger>::Mailbox::iter_key(sender)
                        .map(|(msg, _bn)| msg)
                        .collect();

                log::trace!(
                    "Found {} messages in mailbox: {:?}",
                    messages.len(),
                    messages.iter().map(|msg| msg.id()).collect::<Vec<_>>()
                );

                runtime_data.mailbox_messages = runtime_data
                    .mailbox_messages
                    .drain(..)
                    .chain(messages.into_iter().map(|msg| msg.id()))
                    .unique()
                    .collect();
            }
            _ => {}
        }
    }
}

/// Runs all the fuzz testing internal machinery.
pub fn run(gear_calls: GearCalls) {
    run_impl(gear_calls);
}

fn run_impl(GearCalls(gear_calls): GearCalls) -> sp_io::TestExternalities {
    let sender = runtime::account(runtime::alice());

    let mut test_ext = new_test_ext();
    test_ext.execute_with(|| {
        // Increase maximum balance of the `sender`.
        {
            increase_to_max_balance(sender.clone())
                .unwrap_or_else(|e| unreachable!("Balance update failed: {e:?}"));
            log::info!(
                "Current balance of the sender - {}",
                BalancesPallet::<Runtime>::free_balance(&sender)
            );
        }

        let mut runtime_data = FuzzerRuntimeData::default();
        for gear_call in gear_calls {
            let gear_call = gear_call.preprocess(&runtime_data);
            let call_res = execute_gear_call(sender.clone(), gear_call.clone(), &mut runtime_data);
            log::info!("Extrinsic result: {call_res:?}");

            // Run task and message queues with max possible gas limit.
            run_to_next_block();

            gear_call.apply_runtime_data_update(sender.clone(), &mut runtime_data);
        }
    });

    test_ext
}

fn execute_gear_call(
    sender: AccountId,
    call: GearCall,
    _: &mut FuzzerRuntimeData,
) -> DispatchResultWithPostInfo {
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
            )
        }
        GearCall::SendMessage(args) => {
            let SendMessageArgs((destination, payload, gas_limit, value, prepaid)) = args;
            Gear::send_message(
                RuntimeOrigin::signed(sender),
                destination,
                payload,
                gas_limit,
                value,
                prepaid,
            )
        }
        GearCall::SendReply(args) => {
            let SendReplyArgs((message_id, payload, gas_limit, value, prepaid)) = args;
            Gear::send_reply(
                RuntimeOrigin::signed(sender),
                message_id,
                payload,
                gas_limit,
                value,
                prepaid,
            )
        }
        GearCall::ClaimValue(args) => {
            let ClaimValueArgs(message_id) = args;
            Gear::claim_value(RuntimeOrigin::signed(sender), message_id)
        }
        _ => unimplemented!("Unsupported currently."),
    }
}
