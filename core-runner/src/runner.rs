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

//! Module for running programs.

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::vec::Vec;
use codec::{Decode, Encode};

use gear_backend_common::Environment;
use gear_core::{
    env::Ext as EnvExt,
    gas::{self, GasCounter},
    memory::{MemoryContext, PageNumber},
    message::{
        ExitCode, IncomingMessage, Message, MessageContext, MessageId, MessageIdGenerator,
        MessageState, OutgoingMessage, ReplyMessage,
    },
    program::{Program, ProgramId},
    storage::{InMemoryStorage, ProgramStorage, Storage, StorageCarrier},
};

use crate::builder::RunnerBuilder;
use crate::ext::{BlockInfo, Ext};
use crate::util::BlakeMessageIdGenerator;

/// Runner configuration.
#[derive(Clone, Debug, Decode, Encode)]
pub struct Config {
    /// Total memory pages count.
    pub max_pages: PageNumber,
    /// Gas cost for memory page allocation.
    pub alloc_cost: u64,
    /// Gas cost for memory grow
    pub mem_grow_cost: u64,
    /// Gas cost for init memory page.
    pub init_cost: u64,
    /// Gas cost for loading memory page from program state.
    pub load_page_cost: u64,
}

const EXIT_CODE_PANIC: i32 = 1;

impl Default for Config {
    fn default() -> Self {
        Self {
            max_pages: MAX_PAGES.into(),
            alloc_cost: ALLOC_COST.into(),
            mem_grow_cost: MEM_GROW_COST.into(),
            init_cost: INIT_COST.into(),
            load_page_cost: LOAD_PAGE_COST.into(),
        }
    }
}

impl Config {
    /// Returns config with all costs set to zero
    pub fn zero_cost_config() -> Self {
        Self {
            max_pages: MAX_PAGES.into(),
            alloc_cost: 0,
            mem_grow_cost: 0,
            init_cost: 0,
            load_page_cost: 0,
        }
    }
}

/// Result of one or more message handling.
#[derive(Debug, Default, Clone)]
pub struct RunNextResult {
    /// List of resulting messages.
    pub messages: Vec<Message>,
    /// List of resulting log messages.
    pub log: Vec<Message>,
    /// The ID of the program that has been run.
    pub prog_id: ProgramId,
    /// Execution outcome per each message
    pub outcomes: BTreeMap<MessageId, ExecutionOutcome>,
    /// Gas that was spent.
    ///
    /// Gas that was burned for computations, for each message.
    pub gas_spent: Vec<(MessageId, u64)>,
    /// List of waiting messages.
    pub wait_list: Vec<Message>,
    /// Messages to be waken.
    pub awakening: Vec<MessageId>,
}

impl RunNextResult {
    /// Create an empty `RunNextResult`
    pub(crate) fn new() -> Self {
        Default::default()
    }

    /// Accrue one run of the message hadling.
    pub fn accrue(&mut self, message_id: MessageId, result: RunResult) {
        self.outcomes.insert(message_id, result.outcome);
        self.gas_spent.push((message_id, result.gas_spent));
        self.awakening = result.awakening;
    }

    /// From one single run.
    pub fn from_single(message: Message, run_result: RunResult) -> Self {
        let mut result = Self::new();
        result.prog_id = message.dest;
        let message_id = message.id;
        let run_result = run_result;
        if let ExecutionOutcome::Waiting = run_result.outcome {
            result.wait_list.push(message);
        }

        result.accrue(message_id, run_result);
        result
    }

    /// Has any trap outcome
    pub fn any_traps(&self) -> bool {
        for (_msg_id, outcome) in self.outcomes.iter() {
            if outcome.was_trap() {
                return true;
            }
        }
        false
    }
}

/// Runner instance.
///
/// This instance allows to handle multiple messages using underlying allocation, message and program
/// storage.
#[derive(Default)]
pub struct Runner<SC: StorageCarrier, E: Environment<Ext>> {
    pub(crate) program_storage: SC::PS,
    pub(crate) config: Config,
    env: E,
    block_info: BlockInfo,
    wait_list: BTreeMap<(ProgramId, MessageId), Message>,
}

/// Fully in-memory runner builder (for tests).
pub type InMemoryRunner<E> = Runner<InMemoryStorage, E>;

/// Message payload with pre-generated identifier and economic data.
#[derive(Clone)]
pub struct ExtMessage {
    /// Id of the message.
    pub id: MessageId,
    /// Message payload.
    pub payload: Vec<u8>,
    /// Gas limit for the message dispatch.
    pub gas_limit: u64,
    /// Value associated with the message.
    pub value: u128,
}

/// Program initialization request.
///
/// Program is initialized from some user identity. The identity of the program itself must be known.
/// The initialization message id also must be known in advance (all message chain about program initialization
/// will start from the deterministic message known in advance).
pub struct InitializeProgramInfo {
    /// Identity of the program creator.
    ///
    /// Either user who sends an external transaction or another program.
    pub source_id: ProgramId,
    /// Identity of the new program.
    pub new_program_id: ProgramId,
    /// Initialization message with economic data.
    pub message: ExtMessage,
    /// Code of the new program.
    pub code: Vec<u8>,
}

/// New message dispatch request.
///
/// Message is dispatched from some identity to some identity, both should be known in advance.
#[derive(Clone)]
pub struct MessageDispatch {
    /// Identity of the message origin.
    pub source_id: ProgramId,
    /// Identity of the destination.
    pub destination_id: ProgramId,
    /// Message payload and economic data.
    pub data: ExtMessage,
}

/// New reply dispatch request.
///
/// Reply is dispatched from some identity to some identity, both should be known in advance.
/// Reply also references the message it replies to and have an exit code of what was the dispatch
/// result of that referenced message.
pub struct ReplyDispatch {
    /// Identity of the message origin.
    pub source_id: ProgramId,
    /// Identity of the destination.
    pub destination_id: ProgramId,
    /// Id of the referenced message,
    pub original_message_id: MessageId,
    /// Dispatch result of the referenced message.
    pub original_exit_code: ExitCode,
    /// Message payload and economic data.
    pub data: ExtMessage,
}

impl<SC: StorageCarrier, E: Environment<Ext>> Runner<SC, E> {
    /// New runner instance.
    ///
    /// Provide configuration, storage.
    pub fn new(config: &Config, storage: Storage<SC::PS>, block_info: BlockInfo, env: E) -> Self {
        let Storage { program_storage } = storage;

        Self {
            program_storage,
            config: config.clone(),
            env,
            block_info,
            wait_list: BTreeMap::new(),
        }
    }

    /// Create an empty runner builder.
    pub fn builder() -> RunnerBuilder<SC, E> {
        crate::runner::RunnerBuilder::new()
    }

    /// Run handling next message in the queue.
    ///
    /// Runner will return actual number of messages that was handled.
    /// Messages with no destination won't be handled.
    pub fn run_next(&mut self, message: Message) -> RunNextResult {
        let gas_limit = message.gas_limit();
        let message_source = message.source();
        let message_dest = message.dest();

        let mut program = match self.program_storage.get(message_dest) {
            Some(program) => program,
            None => {
                let mut r = RunNextResult::new();
                r.log.push(message);
                return r;
            }
        };

        let instrumented_code = match gas::instrument(program.code()) {
            Ok(code) => code,
            Err(err) => {
                log::debug!("Instrumentation error: {:?}", err);
                return RunNextResult::new();
            }
        };

        let allocations: BTreeSet<PageNumber> = program
            .get_pages()
            .iter()
            .map(|(page_num, _)| *page_num)
            .collect();

        let mut context = self.create_context(allocations);
        let next_message_id = message.id();

        // We don't generate reply on trap, if we already processing trap message
        let generate_reply_on_trap = if let Some((_, exit_code)) = message.reply() {
            // reply case. generate if not a trap message
            exit_code == 0
        } else {
            // none-reply case. always generate
            true
        };

        let incoming_message: IncomingMessage = message.clone().into();
        let run_result = run(
            &mut self.env,
            &mut context,
            &instrumented_code,
            &mut program,
            if message.reply().is_some() {
                EntryPoint::HandleReply
            } else {
                EntryPoint::Handle
            },
            &incoming_message,
            gas_limit,
            self.block_info,
        );

        let outgoing_messages = context.message_buf.drain(..).collect::<Vec<_>>();
        let mut messages = vec![];
        let mut log = vec![];

        if run_result.outcome.was_trap() && generate_reply_on_trap {
            let gas_spent_for_outgoing: u64 =
                outgoing_messages.iter().map(|msg| msg.gas_limit).sum();
            let burned_gas = run_result.gas_spent;

            let trap_gas = incoming_message
                .gas_limit()
                .saturating_sub(gas_spent_for_outgoing)
                .saturating_sub(burned_gas);

            // In case of trap, we generate trap reply message
            let program_id = program.id();
            let nonce = program.fetch_inc_message_nonce();
            let trap_message_id = self.next_message_id(program_id, nonce);
            let trap_message = Message {
                id: trap_message_id,
                source: program_id,
                dest: message_source,
                payload: vec![].into(),
                gas_limit: trap_gas,
                value: 0,
                reply: Some((next_message_id, EXIT_CODE_PANIC)),
            };

            if self.program_storage.exists(message_source) {
                messages.push(trap_message)
            } else {
                log.push(trap_message)
            }
        }

        let mut result = RunNextResult::from_single(message, run_result);

        for message in outgoing_messages.into_iter() {
            if self.program_storage.exists(message.dest()) {
                messages.push(message);
            } else {
                log.push(message);
            }
        }

        self.program_storage.set(program);
        result.messages.append(&mut messages);
        result.log.append(&mut log);

        result
    }

    /// Process the wait list.
    ///
    /// Use it only for in-memory storage (i.e. for testing purposes).
    pub fn process_wait_list(&mut self, result: &mut RunNextResult) {
        let prog_id = result.prog_id;

        result.wait_list.drain(..).for_each(|msg| {
            self.wait_list.insert((prog_id, msg.id), msg);
        });

        // Messages to be added back to the queue
        let msgs: Vec<_> = result
            .awakening
            .iter()
            .filter_map(|msg_id| self.wait_list.remove(&(prog_id, *msg_id)))
            .collect();

        for msg in msgs {
            result.messages.push(msg);
        }
    }

    /// Drop this runner.
    ///
    /// This will return underlying storage and memory state.
    pub fn complete(self) -> Storage<SC::PS> {
        let Runner {
            program_storage, ..
        } = self;

        Storage { program_storage }
    }

    /// Storage of this runner.
    ///
    /// This will return underlying storage and memory state.
    pub fn storage(&self) -> Storage<SC::PS> {
        Storage {
            program_storage: self.program_storage.clone(),
        }
    }

    /// Max pages configuration of this runner.
    pub fn max_pages(&self) -> PageNumber {
        self.config.max_pages
    }

    /// Gas memory page allocation cost configuration of this runner.
    pub fn alloc_cost(&self) -> u64 {
        self.config.alloc_cost
    }

    /// Gas memory grow cost configuration of this runner.
    pub fn mem_grow_cost(&self) -> u64 {
        self.config.mem_grow_cost
    }

    /// Gas initial memory page cost of this runner.
    pub fn init_cost(&self) -> u64 {
        self.config.init_cost
    }

    /// Gas cost for loading memory page.
    pub fn load_page_cost(&self) -> u64 {
        self.config.load_page_cost
    }

    fn create_context(&self, allocations: BTreeSet<PageNumber>) -> RunningContext {
        RunningContext::new(&self.config, allocations)
    }

    /// Initialize a new program.
    ///
    /// This includes putting this program in the storage and dispatching
    /// initialization message for it.
    ///
    /// Initialization process looks as following:
    /// - The storage is checked for existence of the smart contract with an `initialization.new_program_id` address.
    /// If there is such entry, we reset it with the code from `initialization.code`. If there weren't any entries, we set
    /// a new program to the program storage with empty memory pages data.
    /// - The run of the *init* function is performed.
    /// - The function run can end up with newly created messages, which are added to the storage message queue
    /// or log.
    /// - Running the function in the program can mutate it's state. All the state mutations are at first handled
    /// in memory. If the function run was successful then program storage will be updated with the program with
    /// updated pages data. An update actually can happen while running the *init* function.
    ///
    /// # Errors
    ///
    /// Function returns an error in several situations:
    /// 1. Creating, setting and resetting a [`Program`] ended up with an error.
    /// 2. If needed static pages amount is more then the maximum set in the runner config, function returns with an error.
    /// 3. If code instrumentation with gas instructions ended up with an error.
    pub fn init_program(
        &mut self,
        initialization: InitializeProgramInfo,
    ) -> anyhow::Result<RunResult> {
        if let Some(mut program) = self.program_storage.get(initialization.new_program_id) {
            program.reset(initialization.code)?;
            self.program_storage.set(program);
        } else {
            self.program_storage.set(Program::new(
                initialization.new_program_id,
                initialization.code,
                Default::default(),
            )?);
        }

        let mut program = self
            .program_storage
            .get(initialization.new_program_id)
            .expect("Added above; cannot fail");

        if program.static_pages() > self.max_pages().raw() {
            return Err(anyhow::anyhow!(
                "Error initialisation: memory limit exceeded"
            ));
        }

        let allocations: BTreeSet<PageNumber> = (0..program.static_pages())
            .map(|page| page.into())
            .collect();

        let mut context = self.create_context(allocations);

        let msg = IncomingMessage::new(
            initialization.message.id,
            initialization.source_id,
            initialization.message.payload.into(),
            initialization.message.gas_limit,
            initialization.message.value,
        );

        let res = run(
            &mut self.env,
            &mut context,
            &gas::instrument(program.code())
                .map_err(|e| anyhow::anyhow!("Error instrumenting: {:?}", e))?,
            &mut program,
            EntryPoint::Init,
            &msg,
            initialization.message.gas_limit,
            self.block_info,
        );

        self.program_storage.set(program);

        Ok(res)
    }

    fn next_message_id(&mut self, source: ProgramId, nonce: u64) -> MessageId {
        let mut id_generator = BlakeMessageIdGenerator {
            program_id: source,
            nonce,
        };

        id_generator.next()
    }

    /// Set the block height value.
    pub fn set_block_height(&mut self, value: u32) {
        self.block_info.height = value;
    }

    /// Set the block timestamp.
    pub fn set_block_timestamp(&mut self, value: u64) {
        self.block_info.timestamp = value;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum EntryPoint {
    Handle,
    HandleReply,
    Init,
}

impl From<EntryPoint> for &'static str {
    fn from(entry_point: EntryPoint) -> &'static str {
        match entry_point {
            EntryPoint::Handle => "handle",
            EntryPoint::HandleReply => "handle_reply",
            EntryPoint::Init => "init",
        }
    }
}

static MAX_PAGES: u32 = 512;
static INIT_COST: u32 = 5000;
static ALLOC_COST: u32 = 10000;
static MEM_GROW_COST: u32 = 10000;
static LOAD_PAGE_COST: u32 = 3000;

struct RunningContext {
    config: Config,
    allocations: BTreeSet<PageNumber>,
    message_buf: Vec<Message>,
}

impl RunningContext {
    fn new(config: &Config, allocations: BTreeSet<PageNumber>) -> Self {
        Self {
            config: config.clone(),
            message_buf: vec![],
            allocations,
        }
    }

    fn max_pages(&self) -> PageNumber {
        self.config.max_pages
    }

    pub fn alloc_cost(&self) -> u64 {
        self.config.alloc_cost
    }

    pub fn mem_grow_cost(&self) -> u64 {
        self.config.mem_grow_cost
    }

    fn push_message(&mut self, msg: Message) {
        self.message_buf.push(msg)
    }
}

/// Execution outcome.
///
/// If trap occurred, possible explanation can be attached
#[derive(Clone, Debug)]
pub enum ExecutionOutcome {
    /// Outcome was fine.
    Normal,
    /// Outcome was a trap with some possible explanation.
    Trap(Option<&'static str>),
    /// Execution was interrupted and the message is to be moved to the wait list.
    Waiting,
}

impl Default for ExecutionOutcome {
    fn default() -> Self {
        ExecutionOutcome::Normal
    }
}

impl ExecutionOutcome {
    fn was_trap(&self) -> bool {
        matches!(self, ExecutionOutcome::Trap(_))
    }
}

/// The result of running some program.
#[derive(Clone, Debug, Default)]
pub struct RunResult {
    /// Messages that were generated during the run.
    pub messages: Vec<OutgoingMessage>,
    /// Reply that was received during the run.
    pub reply: Option<ReplyMessage>,
    /// Messages to be woken.
    pub awakening: Vec<MessageId>,
    /// Gas that was spent.
    ///
    /// This actually was is charged for computations/memory costs/etc and will never get refunded.
    pub gas_spent: u64,
    /// Run outcome (trap/success/waiting).
    pub outcome: ExecutionOutcome,
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
    program: &mut Program,
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

#[cfg(test)]
mod tests {
    extern crate wabt;

    use super::*;
    use crate::builder::InMemoryRunnerBuilder;
    use core::convert::TryInto;
    use env_logger::Env;
    use gear_core::storage::InMemoryStorage;

    type TestRunner = InMemoryRunner<gear_backend_wasmtime::WasmtimeEnvironment<Ext>>;

    pub fn new_test_builder(
    ) -> InMemoryRunnerBuilder<gear_backend_wasmtime::WasmtimeEnvironment<Ext>> {
        InMemoryRunnerBuilder::<gear_backend_wasmtime::WasmtimeEnvironment<Ext>>::new()
    }

    fn parse_wat(source: &str) -> Vec<u8> {
        let module_bytes = wabt::Wat2Wasm::new()
            .validate(false)
            .convert(source)
            .expect("failed to parse module")
            .as_ref()
            .to_vec();
        module_bytes
    }

    #[test]
    fn init_logger() {
        env_logger::Builder::from_env(Env::default().default_filter_or("warn"))
            .is_test(true)
            .init();
    }

    #[test]
    fn reply_to_calls_works_and_traps() {
        let wat = r#"
            (module
                (import "env" "gr_reply_to"  (func $gr_reply_to (param i32)))
                (import "env" "memory" (memory 2))
                (export "handle" (func $handle))
                (export "handle_reply" (func $handle))
                (export "init" (func $init))
                (func $handle
                    i32.const 65536
                    call $gr_reply_to
                )
                (func $handle_reply
                    i32.const 65536
                    call $gr_reply_to
                )
                (func $init)
            )"#;

        let (mut runner, results): (TestRunner, _) =
            new_test_builder().program(parse_wat(wat)).build();
        assert!(results.iter().all(|r| r.is_ok()));

        let message = Message {
            id: 1000002.into(),
            source: 1001.into(),
            dest: 1.into(),

            payload: vec![].into(),
            gas_limit: u64::MAX,
            value: 0,
            reply: None,
        };

        assert!(runner.run_next(message).any_traps());

        let msg = vec![
            1, 3, 5, 7, 9, 11, 13, 15, 17, 19, 21, 23, 25, 27, 29, 31, 2, 4, 6, 8, 10, 12, 14, 16,
            18, 20, 22, 24, 26, 28, 30, 32,
        ];

        let message = Message {
            id: 1000003.into(),
            source: 1001.into(),
            dest: 1.into(),
            payload: vec![].into(),
            gas_limit: u64::MAX,
            value: 0,
            reply: Some((MessageId::from_slice(&msg), 0)),
        };

        assert!(!runner.run_next(message).any_traps()); // this is handling of automatic reply when first message was trapped; it will also fail

        let InMemoryStorage {
            program_storage, ..
        } = runner.complete();

        let persisted_program = program_storage
            .get(1.into())
            .expect("Program #1 should exist");

        assert_eq!(
            &persisted_program
                .get_pages()
                .get(&1.into())
                .expect("Page #1 shoud exist")[0..32],
            &msg,
        );
    }

    #[test]
    fn runner_simple() {
        // Sends "ok" on init, then sends back the message it retrieved from the handle
        let wat = r#"
        (module
            (import "env" "gr_read"  (func $read (param i32 i32 i32)))
            (import "env" "gr_send"  (func $send (param i32 i32 i32 i64 i32 i32)))
            (import "env" "gr_size"  (func $size (result i32)))
            (import "env" "memory" (memory 1))
            (data (i32.const 0) "ok")
            (export "handle" (func $handle))
            (export "init" (func $init))
            (func $handle
              (local $var0 i32)
              (local $id i32)
                (i32.store offset=12
                    (get_local $id)
                    (i32.const 1)
                )
              i32.const 0
              call $size
              tee_local $var0
              i32.const 0
              call $read
              i32.const 12
              i32.const 0
              get_local $var0
              i32.const 255
              i32.and
              i64.const 0
              i32.const 32768
              i32.const 40000
              call $send
            )
            (func $init
                (local $id i32)
                (i32.store offset=12
                    (get_local $id)
                    (i32.const 1)
                )
                i32.const 12
                i32.const 0
                i32.const 2
                i64.const 10000000
                i32.const 0
                i32.const 40000
                call $send
              )
          )"#;

        let (mut runner, mut results) = new_test_builder()
            .program(parse_wat(wat))
            .with_init_message(ExtMessage {
                id: 1000001.into(),
                payload: "init".as_bytes().to_vec(),
                gas_limit: u64::MAX,
                value: 0,
            })
            .build();

        assert_eq!(
            results
                .pop()
                .and_then(|r| r.ok())
                .and_then(|mut r| r.messages.pop())
                .map(|m| {
                    let m = m.into_message(1.into());
                    (m.payload().to_vec(), m.dest())
                }),
            Some((b"ok".to_vec(), 1.into()))
        );

        let message = Message {
            id: 1000002.into(),
            source: 1001.into(),
            dest: 1.into(),
            payload: b"test".to_vec().into(),
            gas_limit: u64::MAX,
            value: 0,
            reply: None,
        };

        let run_result = runner.run_next(message);

        assert_eq!(
            run_result
                .messages
                .last()
                .map(|m| (m.payload().to_vec(), m.source(), m.dest())),
            Some((b"test".to_vec(), 1.into(), 1.into()))
        );
    }

    #[test]
    fn runner_allocations() {
        // alloc 1 page in init
        // free page num from message in handle and send it back
        let wat = r#"
        (module
            (import "env" "gr_read"  (func $read (param i32 i32 i32)))
            (import "env" "gr_send"  (func $send (param i32 i32 i32 i64 i32 i32)))
            (import "env" "gr_size"  (func $size (result i32)))
            (import "env" "alloc"  (func $alloc (param i32) (result i32)))
            (import "env" "free"  (func $free (param i32)))
            (import "env" "memory" (memory 1))
            (data (i32.const 0) "ok")
            (export "handle" (func $handle))
            (export "init" (func $init))
            (func $handle
              (local $p i32)
              (local $var0 i32)
              (local $id i32)
              (i32.store offset=12
                (get_local $id)
                (i32.const 1)
              )
              i32.const 0
              call $size
              tee_local $var0
              i32.const 0
              call $read
              i32.const 12
              i32.const 0
              get_local $var0
              i32.const 255
              i32.and
              i64.const 1000000000
              i32.const 32768
              i32.const 40000
              call $send
              i32.const 256
              call $free
            )
            (func $init
              (local $id i32)
              (local $msg_size i32)
              (local $alloc_pages i32)
              (local $pages_offset i32)
              (local.set $pages_offset (call $alloc (i32.const 1)))
              (i32.store offset=12
                (get_local $id)
                (i32.const 1)
              )
              (call $send (i32.const 12) (i32.const 0) (i32.const 2) (i64.const 10000000000) (i32.const 32768) (i32.const 40000))
            )
          )"#;

        let (mut runner, mut results) = new_test_builder().program(parse_wat(wat)).build();

        assert_eq!(
            results
                .pop()
                .and_then(|r| r.ok())
                .and_then(|mut r| r.messages.pop())
                .map(|m| {
                    let m = m.into_message(1.into());
                    (m.payload().to_vec(), m.dest())
                }),
            Some((b"ok".to_vec(), 1.into()))
        );

        // send page num to be freed
        let message = Message {
            id: 1000002.into(),
            source: 1001.into(),
            dest: 1.into(),
            payload: vec![256u32 as _].into(),
            gas_limit: u64::MAX,
            value: 0,
            reply: None,
        };

        let run_result = runner.run_next(message);

        assert_eq!(
            run_result
                .messages
                .last()
                .map(|m| (m.payload().to_vec(), m.source(), m.dest())),
            Some((vec![256u32 as _].into(), 1.into(), 1.into()))
        );
    }

    #[test]
    fn wait() {
        // Call `gr_wait` function
        let wat = r#"
        (module
            (import "env" "gr_wait" (func $gr_wait))
            (import "env" "memory" (memory 1))
            (export "handle" (func $handle))
            (export "init" (func $init))
            (func $handle
                call $gr_wait
                call $gr_wait ;; This call is unreachable due to execution interrupt on previous call
            )
            (func $init)
        )"#;

        let source_id = 1001;
        let dest_id = 1;
        let msg_id: MessageId = 1000001.into();

        let (mut runner, results) = new_test_builder()
            .program(parse_wat(wat))
            .with_source_id(source_id)
            .with_program_id(dest_id)
            .with_init_message(ExtMessage {
                id: 1000001.into(),
                payload: "init".as_bytes().to_vec(),
                gas_limit: u64::MAX,
                value: 0,
            })
            .build();
        assert!(results.iter().all(|r| r.is_ok()));

        let payload = b"Test Wait";

        let message = Message {
            id: msg_id,
            source: source_id.into(),
            dest: 1.into(),
            payload: payload.to_vec().into(),
            gas_limit: 1_000_000,
            value: 0,
            reply: None,
        };

        let mut result = runner.run_next(message);

        let InMemoryStorage { program_storage: _ } = runner.complete();

        let msg = result.wait_list.pop().unwrap();
        assert_eq!(msg.source, source_id.into());
        assert_eq!(msg.dest, dest_id.into());
        assert_eq!(msg.payload(), payload);
    }

    #[test]
    fn gas_available() {
        // Charge 100_000 of gas.
        let wat = r#"
        (module
            (import "env" "gr_send"  (func $send (param i32 i32 i32 i64 i32 i32)))
            (export "handle" (func $handle))
            (import "env" "gr_gas_available" (func $gas_available (result i64)))
            (import "env" "memory" (memory 1))
            (export "init" (func $init))
            (func $handle
                (local $id i32)
                (local $gas_av i32)
                (i32.store offset=12
                    (get_local $id)
                    (i32.const 1001)
                )
                (i64.store offset=18
                    (get_local $gas_av)
                    (call $gas_available)
                )
                (call $send (i32.const 12) (i32.const 18) (i32.const 8) (i64.const 1000) (i32.const 32768) (i32.const 40000))
            )
            (func $init)
        )"#;

        let (mut runner, results) = new_test_builder()
            .program(parse_wat(wat))
            .with_init_message(ExtMessage {
                id: 1000001.into(),
                payload: "init".as_bytes().to_vec(),
                gas_limit: u64::MAX,
                value: 0,
            })
            .build();
        assert!(results.iter().all(|r| r.is_ok()));

        let gas_limit = 1000_000;
        let caller_id = 1001.into();

        let message = Message {
            id: 1000001.into(),
            source: caller_id,
            dest: 1.into(),
            payload: vec![].into(),
            gas_limit: 1_000_000,
            value: 0,
            reply: None,
        };

        let result = runner.run_next(message);
        assert_eq!(result.gas_spent.len(), 1);

        let (gas_available, ..) = result
            .log
            .first()
            .map(|m| (m.payload().to_vec(), m.source(), m.dest()))
            .unwrap();

        let gas_available = gas_available
            .as_slice()
            .try_into()
            .expect("slice with incorrect length");

        assert!(u64::from_le_bytes(gas_available) < gas_limit);
    }

    #[test]
    fn gas_allocations() {
        let wat = r#"
        (module
            (export "handle" (func $handle))
            (import "env" "memory" (memory 1))
            (import "env" "alloc"  (func $alloc (param i32) (result i32)))
            (export "init" (func $init))
            (func $handle
              (local $pages_offset i32)
              (local.set $pages_offset (call $alloc (i32.const 1)))
            )
            (func $init
            )
        )"#;

        let mut runner = TestRunner::default(); //Runner::new(&Config::default(), InMemoryStorage::default());

        let caller_id = 1001.into();

        let init_result = runner
            .init_program(InitializeProgramInfo {
                new_program_id: 1.into(),
                source_id: caller_id,
                code: parse_wat(wat),
                message: ExtMessage {
                    id: 1000001.into(),
                    payload: "init".as_bytes().to_vec(),
                    gas_limit: u64::MAX,
                    value: 0,
                },
            })
            .expect("failed to init program");

        let message = Message {
            id: 1000001.into(),
            source: caller_id,
            dest: 1.into(),
            payload: vec![].into(),
            gas_limit: 1_000_000,
            value: 0,
            reply: None,
        };

        // Charge 1000 of gas for initial memory.
        assert_eq!(init_result.gas_spent, runner.init_cost() * 1);

        let result = runner.run_next(message);

        assert_eq!(
            result.gas_spent[0].1,
            (runner.alloc_cost() + runner.mem_grow_cost()) + runner.load_page_cost() * 1 + 3000
        );

        runner.complete();
    }

    #[test]
    fn spending_with_extra_messages() {
        let wat = r#"
            (module
                (import "env" "gr_send"  (func $send (param i32 i32 i32 i64 i32 i32)))
                (import "env" "memory" (memory 1))
                (export "handle" (func $handle))
                (export "init" (func $init))
                (func $handle
                    (call $send (i32.const 12) (i32.const 18) (i32.const 8) (i64.const 1000000000) (i32.const 32768) (i32.const 40000))
                )
                (func $init)
            )"#;

        let (mut runner, results): (TestRunner, _) = new_test_builder()
            .program(parse_wat(wat))
            .with_source_id(1001)
            .with_program_id(1)
            .with_init_message(ExtMessage {
                id: 1000001.into(),
                payload: vec![],
                gas_limit: u64::MAX,
                value: 0,
            })
            .build();
        assert!(results.iter().all(|r| r.is_ok()));

        let message = Message {
            id: 1000001.into(),
            source: 1001.into(),
            dest: 1.into(),
            payload: vec![].into(),
            gas_limit: 2_000_000_000,
            value: 0,
            reply: None,
        };

        let run_result = runner.run_next(message);

        assert_eq!(run_result.gas_spent[0].1, 10_000);
    }
}
