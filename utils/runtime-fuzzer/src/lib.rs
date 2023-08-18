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

mod runtime;

use frame_support::pallet_prelude::DispatchResultWithPostInfo;
use gear_call_gen::{GearCall, GearCalls, SendMessageArgs, UploadProgramArgs};
use gear_runtime::{AccountId, Gear, Runtime, RuntimeOrigin};
use pallet_balances::Pallet as BalancesPallet;
use runtime::*;

/// Runs all the fuzz testing internal machinery.
pub fn run(GearCalls(gear_calls): GearCalls) {
    let sender = runtime::account(runtime::alice());

    new_test_ext().execute_with(|| {
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
            let call_res = execute_gear_call(sender.clone(), gear_call);
            log::info!("Extrinsic result: {call_res:?}");

            // Run task and message queues with max possible gas limit.
            run_to_next_block();
        }
    });
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
        _ => unimplemented!("Unsupported currently."),
    }
}
