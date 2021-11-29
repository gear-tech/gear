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

use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) fn run_init<E: Environment<Ext>>(
    env: &mut E,
    context: &mut RunningContext,
    binary: &[u8],
    program: &mut UninitializedProgram,
    message: &IncomingMessage,
    gas_limit: u64,
    block_info: BlockInfo,
) -> RunResult {
    run(
        env,
        context,
        binary,
        program,
        EntryPoint::Init,
        message,
        gas_limit,
        block_info,
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn run_next<E: Environment<Ext>>(
    env: &mut E,
    context: &mut RunningContext,
    binary: &[u8],
    program: &mut InitializedProgram,
    message: Message,
    gas_limit: u64,
    block_info: BlockInfo,
) -> RunResult {
    run(
        env,
        context,
        binary,
        program,
        if message.reply().is_some() {
            EntryPoint::HandleReply
        } else {
            EntryPoint::Handle
        },
        &message.into(),
        gas_limit,
        block_info,
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn run_reply<E: Environment<Ext>>(
    env: &mut E,
    context: &mut RunningContext,
    binary: &[u8],
    program: &mut UninitializedProgram,
    message: IncomingMessage,
    gas_limit: u64,
    block_info: BlockInfo,
) -> RunResult {
    run(
        env,
        context,
        binary,
        program,
        EntryPoint::HandleReply,
        &message,
        gas_limit,
        block_info,
    )
}

/// Performs run of the `entry_point` function in the `program`.
///
/// The function is needed to abstract common procedures of different program function calls.
///
/// Actual function run is performed in the virtual machine (VM). Programs, which are run in the VM, import functions from some environment
/// that Gear provides. These functions (so called sys-calls), are provided by sandbox or wasmtime backends (see core-backend crates),
/// which implement [`Environment`] trait.
/// This trait provides us an ability to setup all the needed settings for the run and actually run the desired function, providing program (wasm module) with
/// sys-calls.
/// A crucial dependency for the actual run in the VM is `Ext`, which is created in the function's body.
///
/// By the end of the run all the side effects (changes in memory, newly generated messages) are handled.
///
/// The function doesn't return an error, although the run can end up with a trap. However,
/// in the `RunResult.outcome` field we state, that the trap occurred. So the trap occurs in several situations:
/// 1. Gas charge for initial or loaded pages failed;
/// 2. There weren't enough gas for future memory grow;
/// 3. Program function execution ended up with an error.
#[allow(clippy::too_many_arguments)]
fn run<E: Environment<Ext>>(
    env: &mut E,
    context: &mut RunningContext,
    binary: &[u8],
    program: &mut Data,
    entry_point: EntryPoint,
    message: &IncomingMessage,
    gas_limit: u64,
    block_info: BlockInfo,
) -> RunResult {
    let mut gas_counter = GasCounter::new(gas_limit);

    let id_generator = BlakeMessageIdGenerator {
        program_id: program.id(),
        nonce: program.message_nonce(),
    };

    let (left_before, burned_before) = (gas_counter.left(), gas_counter.burned());

    // Charge gas for initial or loaded pages.
    match entry_point {
        EntryPoint::Init => {
            if gas_counter.charge(context.config.init_cost * program.static_pages() as u64)
                == gas::ChargeResult::NotEnough
            {
                return RunResult {
                    outcome: ExecutionOutcome::Trap(Some("Not enough gas for initial memory.")),
                    ..Default::default()
                };
            }
        }
        _ => {
            if gas_counter.charge(context.config.load_page_cost * program.get_pages().len() as u64)
                == gas::ChargeResult::NotEnough
            {
                return RunResult {
                    outcome: ExecutionOutcome::Trap(Some("Not enough gas for loading memory.")),
                    ..Default::default()
                };
            }
        }
    };

    let memory = env.create_memory(program.static_pages());

    // Charge gas for feature memory grows.
    let max_page = program.get_pages().iter().next_back();
    if let Some(max_page) = max_page {
        let max_page_num = *max_page.0;
        let mem_size = memory.size();
        if max_page_num >= mem_size {
            let amount =
                context.config.mem_grow_cost * ((max_page_num - mem_size).raw() as u64 + 1);
            let res = gas_counter.charge(amount);
            if res != gas::ChargeResult::Enough {
                return RunResult {
                    outcome: ExecutionOutcome::Trap(Some("Not enough gas for grow memory size.")),
                    ..Default::default()
                };
            }
        } else {
            assert!(max_page_num.raw() == mem_size.raw() - 1);
        }
    }

    let ext = Ext {
        memory_context: MemoryContext::new(
            program.id(),
            memory.clone(),
            context.allocations.clone(),
            program.static_pages().into(),
            context.max_pages(),
        ),
        messages: MessageContext::new(message.clone(), id_generator),
        gas_counter,
        alloc_cost: context.alloc_cost(),
        mem_grow_cost: context.mem_grow_cost(),
        last_error_returned: None,
        wait_flag: false,
        block_info,
    };

    // Actually runs the `entry_point` function in `binary`. Because of the fact
    // that contracts can use host functions, that are exported to the module (i.e. important by module),
    // these functions can need some data to operate on. This data along with some internal procedures
    // implementing host functions are provided with `ext`.
    let (res, mut ext) = env.setup_and_run(
        ext,
        binary,
        program.get_pages(),
        &*memory,
        entry_point.into(),
    );

    let outcome = match res {
        Ok(_) => {
            if ext.wait_flag {
                ExecutionOutcome::Waiting
            } else {
                ExecutionOutcome::Normal
            }
        }
        Err(e) => {
            let explanation = ext.last_error_returned.take();
            log::debug!(
                "Trap during execution: {}, explanation: {}",
                e,
                explanation.unwrap_or("N/A")
            );
            ExecutionOutcome::Trap(explanation)
        }
    };

    // Handling side effects after running program, which requires:
    // 1. setting newest memory pages for a program
    // 2. Gathering newly generated messages ("outgoing" and reply messages). They are later
    // set to the storage.
    // 3. Transferring remain gas after current run to woken messages.

    // get allocated pages
    for page in ext.memory_context.allocations().clone() {
        let mut buf = vec![0u8; PageNumber::size()];
        ext.get_mem(page.offset(), &mut buf);
        let _ = program.set_page(page, &buf);
    }

    let mut messages = vec![];

    program.set_message_nonce(ext.messages.nonce());
    let MessageState {
        outgoing,
        reply,
        awakening,
    } = ext.messages.into_state();

    for outgoing_msg in outgoing {
        messages.push(outgoing_msg.clone());
        context.push_message(outgoing_msg.into_message(program.id()));
    }

    if let Some(reply_message) = &reply {
        context.push_message(reply_message.clone().into_message(
            message.id(),
            program.id(),
            message.source(),
        ));
    }

    let gas_spent = ext.gas_counter.burned();

    let (left_after, burned_after) = (ext.gas_counter.left(), ext.gas_counter.burned());
    assert!(left_before >= left_after);
    assert!(burned_after >= burned_before);
    log::debug!(
        "({}) Gas burned: {}; Gas used {}",
        program.id(),
        burned_after - burned_before,
        left_before - left_after
    );

    RunResult {
        messages,
        reply,
        awakening,
        gas_spent,
        outcome,
    }
}
