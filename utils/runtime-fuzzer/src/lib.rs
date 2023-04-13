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
use gear_call_gen::{CallGenRng, GearCall, GearProgGenConfig, SendMessageArgs, UploadProgramArgs};
use gear_common::event::ProgramChangeKind;
use gear_core::ids::ProgramId;
use gear_runtime::{AccountId, Gear, Runtime, RuntimeEvent, RuntimeOrigin, System};
use gear_utils::NonEmpty;
use once_cell::sync::OnceCell;
use pallet_balances::Pallet as BalancesPallet;
use pallet_gear::Event;
use parking_lot::Mutex;
use rand::rngs::SmallRng;
use runtime::*;
use sp_io::TestExternalities;

type ContextMutex = Mutex<Context>;

// Saving ext is planned to be multithreaded, so sync primitive is used
// TODO #2189
static TEST_EXT: OnceCell<Mutex<TestExternalities>> = OnceCell::new();
static CONTEXT: OnceCell<Mutex<Context>> = OnceCell::new();

struct Context {
    programs: Vec<ProgramId>,
}

impl Context {
    fn new() -> Self {
        Self {
            programs: Vec::new(),
        }
    }
}

/// Runs all the fuzz testing internal machinery.
pub fn run(seed: u64) {
    let sender = runtime::account(runtime::alice());
    let test_ext = TEST_EXT.get_or_init(|| Mutex::new(new_test_ext()));
    let context = CONTEXT.get_or_init(|| Mutex::new(Context::new()));

    test_ext.lock().execute_with(|| {
        // Increase maximum balance of the `sender`.
        {
            increase_to_max_balance(sender.clone())
                .unwrap_or_else(|e| unreachable!("Balance update failed: {e:?}"));
            log::info!(
                "Current balance of the sender - {}",
                BalancesPallet::<Runtime>::free_balance(&sender)
            );
        }

        // Generate gear call.
        let call = generate_gear_call::<SmallRng>(seed, context);

        // Execute gear call.
        let call_res = execute_gear_call(sender, call);
        log::info!("Extrinsic result: {call_res:?}");

        // Run task and message queues with max possible gas limit.
        run_to_next_block();

        // Update context after the run.
        update_context(context)
    });
}

fn generate_gear_call<Rng: CallGenRng>(seed: u64, context: &ContextMutex) -> GearCall {
    let config = fuzzer_config();
    let mut rand = Rng::seed_from_u64(seed);
    let programs = context.lock().programs.clone();

    match rand.gen_range(0..=1) {
        0 => UploadProgramArgs::generate::<Rng>(
            rand.next_u64(),
            rand.next_u64(),
            default_gas_limit(),
            config,
            programs,
        )
        .into(),
        1 => match NonEmpty::from_vec(context.lock().programs.clone()) {
            Some(existing_programs) => SendMessageArgs::generate::<Rng>(
                existing_programs,
                rand.next_u64(),
                default_gas_limit(),
            )
            .into(),
            None => UploadProgramArgs::generate::<Rng>(
                rand.next_u64(),
                rand.next_u64(),
                default_gas_limit(),
                config,
                programs,
            )
            .into(),
        },
        _ => unreachable!("Generate in range 0..=1."),
    }
}

fn fuzzer_config() -> GearProgGenConfig {
    let mut config = GearProgGenConfig::new_normal();
    config.remove_recursion = (1, 1).into();
    config.call_indirect_enabled = false;

    config
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
            let SendMessageArgs((destination, payload, gas_limit, value)) = args;
            Gear::send_message(
                RuntimeOrigin::signed(sender),
                destination,
                payload,
                gas_limit,
                value,
            )
        }
        _ => unreachable!("Unsupported currently."),
    }
}

fn update_context(context: &ContextMutex) {
    log::debug!("Starting updating context");
    let mut initialized_programs: Vec<_> = System::events()
        .into_iter()
        .filter_map(|v| {
            if let RuntimeEvent::Gear(Event::ProgramChanged {
                id,
                change: ProgramChangeKind::Active { .. },
            }) = v.event
            {
                Some(id)
            } else {
                None
            }
        })
        .collect();

    System::reset_events();

    log::debug!("Collected all the programs");

    context.lock().programs.append(&mut initialized_programs);
    log::debug!("Context is stored");
}
