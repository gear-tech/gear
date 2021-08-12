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

use alloc::boxed::Box;
use alloc::collections::{BTreeMap, BTreeSet};
use alloc::string::String;
use alloc::vec::Vec;
use codec::{Decode, Encode};

use gear_core::storage::WaitList;
use gear_core::{
    env::Ext as EnvExt,
    gas::{self, ChargeResult, GasCounter, GasCounterLimited},
    memory::{Memory, MemoryContext, PageNumber},
    message::{
        ExitCode, IncomingMessage, Message, MessageContext, MessageId, MessageIdGenerator,
        OutgoingMessage, OutgoingPacket, ReplyMessage, ReplyPacket,
    },
    program::{Program, ProgramId},
    storage::{MessageQueue, ProgramStorage, Storage},
};

use gear_core_backend::Environment;

/// Runner configuration.
#[derive(Clone, Debug, Decode, Encode)]
pub struct Config {
    /// Total memory pages count.
    pub max_pages: PageNumber,
    /// Gas cost for memory page allocation.
    pub alloc_cost: u64,
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
            init_cost: INIT_COST.into(),
            load_page_cost: LOAD_PAGE_COST.into(),
        }
    }
}

type GasRequest = (ProgramId, ProgramId, u64);

/// Result of one or more message handling.
#[derive(Debug, Default, Clone)]
pub struct RunNextResult {
    /// How many messages were handled
    pub handled: u32,
    /// Execution outcome per each message
    pub outcomes: BTreeMap<MessageId, ExecutionOutcome>,
    /// Gas that was left.
    pub gas_left: Vec<(ProgramId, u64)>,
    /// Gas that was spent.
    pub gas_spent: Vec<(ProgramId, u64)>,
    /// Gas transfer requests.
    pub gas_requests: Vec<GasRequest>,
}

impl RunNextResult {
    /// Result that notes that some log message had been handled, otherwise empty.
    pub(crate) fn log() -> Self {
        RunNextResult {
            handled: 1,
            ..Default::default()
        }
    }

    /// Result that notes that some failed program has been tried to run but nothing really happened.
    pub(crate) fn trap() -> Self {
        RunNextResult {
            handled: 1,
            ..Default::default()
        }
    }

    /// Request all the gas to be reserved for the destination
    pub(crate) fn refund(gas_request: GasRequest) -> Self {
        RunNextResult {
            handled: 1,
            gas_requests: vec![gas_request],
            ..Default::default()
        }
    }

    /// Accrue one run of the message hadling.
    pub fn accrue(
        &mut self,
        message_id: MessageId,
        caller_id: ProgramId,
        program_id: ProgramId,
        result: RunResult,
    ) {
        self.handled += 1;
        self.outcomes.insert(message_id, result.outcome);
        // Report caller's left and spent gas
        self.gas_left.push((caller_id, result.gas_left));
        self.gas_spent.push((caller_id, result.gas_spent));
        if result.gas_requested > 0 {
            // Report that program requested gas transfer
            self.gas_requests
                .push((caller_id, program_id, result.gas_requested));
        }
    }

    /// Empty run result.
    pub fn empty() -> Self {
        RunNextResult::default()
    }

    /// From one single run.
    pub fn from_single(
        message_id: MessageId,
        caller_id: ProgramId,
        program_id: ProgramId,
        run_result: RunResult,
    ) -> Self {
        let mut result = Self::empty();
        result.accrue(message_id, caller_id, program_id, run_result);
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

/// Blake2 Message Id Generator
pub struct BlakeMessageIdGenerator {
    program_id: ProgramId,
    nonce: u64,
}

impl gear_core::message::MessageIdGenerator for BlakeMessageIdGenerator {
    fn next(&mut self) -> MessageId {
        let mut data = self.program_id.as_slice().to_vec();
        data.extend(&self.nonce.to_le_bytes());

        self.nonce += 1;

        MessageId::from_slice(blake2_rfc::blake2b::blake2b(32, &[], &data).as_bytes())
    }

    fn current(&self) -> u64 {
        self.nonce
    }
}

/// Runner instance.
///
/// This instance allows to handle multiple messages using underlying allocation, message and program
/// storage.
pub struct Runner<MQ: MessageQueue, PS: ProgramStorage, WL: WaitList> {
    pub(crate) program_storage: PS,
    pub(crate) message_queue: MQ,
    pub(crate) wait_list: WL,
    pub(crate) config: Config,
    env: Environment<Ext>,
}

/// Message payload with pre-generated identifier and economic data.
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
/// Program is initializaed from some user identity. The identity of the program itself must be known.
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
pub struct MessageDispatch {
    /// Identity of the message origin.
    pub source_id: ProgramId,
    /// Identity of the destination.
    pub destination_id: ProgramId,
    /// Message payload and economic data.
    pub data: ExtMessage,
}

impl MessageDispatch {
    fn into_message(self) -> Message {
        Message::new(
            self.data.id,
            self.source_id,
            self.destination_id,
            self.data.payload.into(),
            self.data.gas_limit,
            self.data.value,
        )
    }
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
    /// Id of the referrenced message,
    pub original_message_id: MessageId,
    /// Dispatch result of the referenced message.
    pub original_exit_code: ExitCode,
    /// Message payload and economic data.
    pub data: ExtMessage,
}

impl ReplyDispatch {
    fn into_message(self) -> Message {
        Message::new_reply(
            self.data.id,
            self.source_id,
            self.destination_id,
            self.data.payload.into(),
            self.data.gas_limit,
            self.data.value,
            self.original_message_id,
            self.original_exit_code,
        )
    }
}

impl<MQ: MessageQueue, PS: ProgramStorage, WL: WaitList> Runner<MQ, PS, WL> {
    /// New runner instance.
    ///
    /// Provide configuration, storage.
    pub fn new(config: &Config, storage: Storage<MQ, PS, WL>) -> Self {
        let env = Environment::new();

        let Storage {
            message_queue,
            program_storage,
            wait_list,
        } = storage;

        Self {
            program_storage,
            message_queue,
            wait_list,
            config: config.clone(),
            env,
        }
    }

    /// Run handling next message in the queue.
    ///
    /// Runner will return actual number of messages that was handled.
    /// Messages with no destination won't be handled.
    pub fn run_next(&mut self, max_gas_limit: u64) -> RunNextResult {
        let next_message = match self.message_queue.dequeue() {
            Some(msg) => msg,
            None => {
                return RunNextResult::empty();
            }
        };

        if next_message.dest() == 0.into() {
            match String::from_utf8(next_message.payload().to_vec()) {
                Ok(s) => log::debug!("UTF-8 msg to /0: {}", s),
                Err(_) => {
                    log::debug!("msg to /0: {:?}", next_message.payload())
                }
            }
            RunNextResult::log()
        } else {
            let gas_limit = next_message.gas_limit();
            let next_message_source = next_message.source();
            let next_message_dest = next_message.dest();

            let mut program = match self.program_storage.get(next_message_dest) {
                Some(program) => program,
                None => {
                    // Reserve the entire `gas_limit` so that it is transferred to the addressee eventually
                    return RunNextResult::refund((
                        next_message_source,
                        next_message_dest,
                        gas_limit,
                    ));
                }
            };

            if gas_limit > max_gas_limit {
                // Re-queue the message to be processed in one of the following blocks
                log::info!(
                    "Message gas limit of {} exceeds the remaining block gas allowance of {}",
                    gas_limit,
                    max_gas_limit
                );
                self.message_queue.queue(next_message);
                return RunNextResult::empty();
            }

            let instrumeted_code = match gas::instrument(program.code()) {
                Ok(code) => code,
                Err(err) => {
                    log::debug!("Instrumentation error: {:?}", err);
                    return RunNextResult::trap();
                }
            };

            let allocations: BTreeSet<PageNumber> = program
                .get_pages()
                .iter()
                .map(|(page_num, _)| *page_num)
                .collect();

            let mut context = self.create_context(allocations);
            let next_message_id = next_message.id();

            let mut run_result = run(
                &mut self.env,
                &mut context,
                &instrumeted_code,
                &mut program,
                if next_message.reply().is_some() {
                    EntryPoint::HandleReply
                } else {
                    EntryPoint::Handle
                },
                &next_message.into(),
                gas_limit,
            );

            if run_result.outcome.was_trap() {
                // In case of trap, we generate trap reply message
                let program_id = program.id();
                let nonce = program.fetch_inc_message_nonce();
                let trap_message_id = self.next_message_id(program_id, nonce);

                self.message_queue.queue(Message {
                    id: trap_message_id,
                    source: program_id,
                    dest: next_message_source,
                    payload: vec![].into(),
                    gas_limit: run_result.gas_left,
                    value: 0,
                    reply: Some((next_message_id, EXIT_CODE_PANIC)),
                });
            }

            if let Some(waiting_msg) = run_result.waiting.take() {
                self.wait_list.insert(waiting_msg.id, waiting_msg);
            }

            if let Some((gas, waker_id)) = run_result.awakening {
                if let Some(mut msg) = self.wait_list.remove(waker_id) {
                    // Increase gas available to the message
                    msg.gas_limit += gas;
                    context.message_buf.push(msg);
                }
            }

            let result = RunNextResult::from_single(
                next_message_id,
                next_message_source,
                next_message_dest,
                run_result,
            );

            self.message_queue
                .queue_many(context.message_buf.drain(..).collect());
            self.program_storage.set(program);

            result
        }
    }

    /// Drop this runner.
    ///
    /// This will return underlyign storage and memory state.
    pub fn complete(self) -> Storage<MQ, PS, WL> {
        let Runner {
            program_storage,
            message_queue,
            wait_list,
            ..
        } = self;

        Storage {
            message_queue,
            program_storage,
            wait_list,
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

    /// Initialize new program.
    ///
    /// This includes putting this program in the storage and dispatching
    /// initializationg message for it.
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
        );

        self.message_queue
            .queue_many(context.message_buf.drain(..).collect());
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

    /// Queue message for the underlying message queue.
    pub fn queue_message(&mut self, dispatch: MessageDispatch) {
        self.message_queue.queue(dispatch.into_message());
    }

    /// Queue a reply message for the underlying message queue.
    pub fn queue_reply(&mut self, dispatch: ReplyDispatch) {
        self.message_queue.queue(dispatch.into_message());
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

    pub fn init_cost(&self) -> u64 {
        self.config.init_cost
    }

    pub fn load_page_cost(&self) -> u64 {
        self.config.load_page_cost
    }

    fn push_message(&mut self, msg: Message) {
        self.message_buf.push(msg)
    }
}

/// Execution outcome.
///
/// If trap occured, possible explanation can be attached
#[derive(Clone, Debug)]
pub enum ExecutionOutcome {
    /// Outcome was a trap with some possible explanation.
    Trap(Option<&'static str>),
    /// Outcome was fine.
    Normal,
}

impl Default for ExecutionOutcome {
    fn default() -> Self {
        ExecutionOutcome::Normal
    }
}

impl ExecutionOutcome {
    fn was_trap(&self) -> bool {
        match self {
            ExecutionOutcome::Trap(_) => true,
            ExecutionOutcome::Normal => false,
        }
    }
}

/// The result of running some program.
#[derive(Clone, Debug, Default)]
pub struct RunResult {
    /// Messages that were generated during the run.
    pub messages: Vec<OutgoingMessage>,
    /// Reply that was received during the run.
    pub reply: Option<ReplyMessage>,
    /// Message to be added to the wait list.
    pub waiting: Option<Message>,
    /// Message to be woken.
    pub awakening: Option<(u64, MessageId)>,
    /// Gas that was left.
    pub gas_left: u64,
    /// Gas that was spent.
    pub gas_spent: u64,
    /// Gas requested to be transferred.
    pub gas_requested: u64,
    /// Run outcome (trap/succes).
    pub outcome: ExecutionOutcome,
}

struct Ext {
    memory_context: MemoryContext,
    messages: MessageContext<BlakeMessageIdGenerator>,
    gas_counter: Box<dyn GasCounter>,
    gas_requested: u64,
    alloc_cost: u64,
    last_error_returned: Option<&'static str>,
}

impl Ext {
    fn return_with_tracing<T>(
        &mut self,
        result: Result<T, &'static str>,
    ) -> Result<T, &'static str> {
        match result {
            Ok(result) => Ok(result),
            Err(error_string) => {
                self.last_error_returned = Some(error_string);
                Err(error_string)
            }
        }
    }
}

impl EnvExt for Ext {
    fn alloc(&mut self, pages: PageNumber) -> Result<PageNumber, &'static str> {
        self.gas(pages.raw() * self.alloc_cost as u32)?;
        let result = self
            .memory_context
            .alloc(pages)
            .map_err(|_e| "Allocation error");

        self.return_with_tracing(result)
    }

    fn send(&mut self, msg: OutgoingPacket) -> Result<(), &'static str> {
        if self.gas_counter.charge(msg.gas_limit()) != ChargeResult::Enough {
            return Err("Gas limit exceeded while trying to send message");
        }
        let result = self.messages.send(msg).map_err(|_e| "Message send error");

        self.return_with_tracing(result)
    }

    fn send_init(&mut self, msg: OutgoingPacket) -> Result<usize, &'static str> {
        let result = self
            .messages
            .send_init(msg)
            .map_err(|_e| "Message init error");

        self.return_with_tracing(result)
    }

    fn send_push(&mut self, handle: usize, buffer: &[u8]) -> Result<(), &'static str> {
        let result = self
            .messages
            .send_push(handle, buffer)
            .map_err(|_e| "Payload push error");

        self.return_with_tracing(result)
    }

    fn reply_push(&mut self, buffer: &[u8]) -> Result<(), &'static str> {
        let result = self
            .messages
            .reply_push(buffer)
            .map_err(|_e| "Reply payload push error");

        self.return_with_tracing(result)
    }

    fn send_commit(&mut self, handle: usize) -> Result<(), &'static str> {
        {
            let gas_limit = match self
                .messages
                .get_gas_limit(handle)
                .map_err(|_e| "Message commit error")
            {
                Ok(gas) => gas,
                anything_else => {
                    return self.return_with_tracing(anything_else.map(|_never| ()));
                }
            };

            if self.gas_counter.charge(gas_limit) != ChargeResult::Enough {
                return self
                    .return_with_tracing(Err("Gas limit exceeded while trying to send message"));
            }
        }

        let result = self
            .messages
            .send_commit(handle)
            .map_err(|_e| "Message commit error");

        self.return_with_tracing(result)
    }

    fn reply(&mut self, msg: ReplyPacket) -> Result<(), &'static str> {
        let result = self.messages.reply(msg).map_err(|_e| "Reply error");

        self.return_with_tracing(result)
    }

    fn reply_to(&self) -> Option<(MessageId, ExitCode)> {
        self.messages.current().reply()
    }

    fn source(&mut self) -> ProgramId {
        self.messages.current().source()
    }

    fn message_id(&mut self) -> MessageId {
        self.messages.current().id()
    }

    fn free(&mut self, ptr: PageNumber) -> Result<(), &'static str> {
        let result = self.memory_context.free(ptr).map_err(|_e| "Free error");
        self.return_with_tracing(result)
    }

    fn debug(&mut self, data: &str) -> Result<(), &'static str> {
        log::debug!("DEBUG: {}", data);
        Ok(())
    }

    fn set_mem(&mut self, ptr: usize, val: &[u8]) {
        self.memory_context
            .memory()
            .write(ptr, val)
            // TODO: remove and propagate error, issue #97
            .expect("Memory out of bounds.");
    }

    fn get_mem(&self, ptr: usize, buffer: &mut [u8]) {
        self.memory_context.memory().read(ptr, buffer);
    }

    fn msg(&mut self) -> &[u8] {
        self.messages.current().payload()
    }

    fn gas(&mut self, val: u32) -> Result<(), &'static str> {
        if self.gas_counter.charge(val as u64) == ChargeResult::Enough {
            Ok(())
        } else {
            self.return_with_tracing(Err("Gas limit exceeded"))
        }
    }

    fn gas_available(&mut self) -> u64 {
        self.gas_counter.left()
    }

    fn charge(&mut self, gas: u64) -> Result<(), &'static str> {
        if self.gas_counter.charge(gas) == ChargeResult::Enough {
            self.gas_requested += gas;
            Ok(())
        } else {
            self.return_with_tracing(Err("Gas limit exceeded"))
        }
    }

    fn value(&self) -> u128 {
        self.messages.current().value()
    }

    fn wait(&mut self) -> Result<(), &'static str> {
        let result = self
            .messages
            .wait()
            .map_err(|_| "Unable to add the message to the wait list");

        self.return_with_tracing(result)
    }

    fn wake(&mut self, waker_id: MessageId) -> Result<(), &'static str> {
        let result = self
            .messages
            .wake(waker_id)
            .map_err(|_| "Unable to mark the message to be woken");

        self.return_with_tracing(result)
    }
}

fn run(
    env: &mut Environment<Ext>,
    context: &mut RunningContext,
    binary: &[u8],
    program: &mut Program,
    entry_point: EntryPoint,
    message: &IncomingMessage,
    gas_limit: u64,
) -> RunResult {
    let mut gas_counter = Box::new(GasCounterLimited(gas_limit)) as Box<dyn GasCounter>;

    let id_generator = BlakeMessageIdGenerator {
        program_id: program.id(),
        nonce: program.message_nonce(),
    };

    // Charge gas for initial or loaded pages.
    match entry_point {
        EntryPoint::Init => {
            if gas_counter.charge(context.config.init_cost * program.static_pages() as u64)
                == gas::ChargeResult::NotEnough
            {
                let gas_left = gas_counter.left();
                return RunResult {
                    messages: vec![],
                    reply: None,
                    waiting: None,
                    awakening: None,
                    gas_left,
                    gas_spent: 0,
                    gas_requested: 0,
                    outcome: ExecutionOutcome::Trap(Some("Not enough gas for initial memory.")),
                };
            }
        }
        _ => {
            if gas_counter.charge(context.config.load_page_cost * program.get_pages().len() as u64)
                == gas::ChargeResult::NotEnough
            {
                let gas_left = gas_counter.left();
                return RunResult {
                    messages: vec![],
                    reply: None,
                    waiting: None,
                    awakening: None,
                    gas_left,
                    gas_spent: 0,
                    gas_requested: 0,
                    outcome: ExecutionOutcome::Trap(Some("Not enough gas for loading memory.")),
                };
            }
        }
    };

    let memory = env.create_memory(program.static_pages());

    let ext = Ext {
        memory_context: MemoryContext::new(
            program.id(),
            Memory::clone(&memory),
            context.allocations.clone(),
            program.static_pages().into(),
            context.max_pages(),
        ),
        messages: MessageContext::new(message.clone(), id_generator),
        gas_counter,
        gas_requested: 0,
        alloc_cost: context.alloc_cost(),
        last_error_returned: None,
    };

    let (res, mut ext) = env.setup_and_run(
        ext,
        binary,
        program.get_pages(),
        &memory,
        entry_point.into(),
    );

    let outcome = match res {
        Ok(_) => ExecutionOutcome::Normal,
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

    // get allocated pages
    for page in ext.memory_context.allocations().clone() {
        let mut buf = vec![0u8; PageNumber::size()];
        ext.get_mem(page.offset(), &mut buf);
        let _ = program.set_page(page, &buf);
    }

    let mut messages = vec![];

    program.set_message_nonce(ext.messages.nonce());
    let (outgoing, reply, waiting, awakening) = ext.messages.drain();

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

    let mut gas_left = ext.gas_counter.left();
    let gas_requested = ext.gas_requested;
    let gas_spent = gas_limit - gas_left - gas_requested;

    let waiting = waiting.map(|mut msg| {
        // Update gas limit according to gas already spent
        msg.set_gas_limit(gas_left);
        // Keep user's balance reserved until message will be really processed
        gas_left = 0;
        msg.into_message(program.id())
    });

    let awakening = awakening.map(|id| {
        let gas_available = gas_left;
        // Transfer current messages's gas to the woken message
        gas_left = 0;
        (gas_available, id)
    });

    RunResult {
        messages,
        reply,
        waiting,
        awakening,
        gas_left,
        gas_spent,
        gas_requested,
        outcome,
    }
}

#[cfg(test)]
mod tests {
    extern crate wabt;

    use super::*;
    use env_logger::Env;
    use gear_core::storage::{
        InMemoryMessageQueue, InMemoryProgramStorage, InMemoryStorage, InMemoryWaitList, MessageMap,
    };

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

    fn new_test_runner() -> Runner<InMemoryMessageQueue, InMemoryProgramStorage, InMemoryWaitList> {
        Runner::new(&Config::default(), Default::default())
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

        let mut runner = new_test_runner();

        runner
            .init_program(InitializeProgramInfo {
                new_program_id: 1.into(),
                source_id: 1001.into(),
                code: parse_wat(wat),
                message: ExtMessage {
                    id: 1000001.into(),
                    payload: vec![],
                    gas_limit: u64::MAX,
                    value: 0,
                },
            })
            .expect("failed to init program");

        runner.queue_message(MessageDispatch {
            source_id: 1001.into(),
            destination_id: 1.into(),
            data: ExtMessage {
                id: 1000002.into(),
                payload: vec![],
                gas_limit: u64::MAX,
                value: 0,
            },
        });

        assert!(runner.run_next(u64::MAX).any_traps());

        let msg = vec![
            1, 3, 5, 7, 9, 11, 13, 15, 17, 19, 21, 23, 25, 27, 29, 31, 2, 4, 6, 8, 10, 12, 14, 16,
            18, 20, 22, 24, 26, 28, 30, 32,
        ];

        runner.queue_reply(ReplyDispatch {
            source_id: 1001.into(),
            destination_id: 1.into(),
            original_message_id: MessageId::from_slice(&msg),
            original_exit_code: 0,
            data: ExtMessage {
                id: 1000003.into(),
                payload: vec![],
                gas_limit: u64::MAX,
                value: 0,
            },
        });

        assert!(!runner.run_next(u64::MAX).any_traps()); // this is handling of automatic reply when first message was trapped; it will also fail
        runner.run_next(u64::MAX);

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
            (import "env" "gr_send"  (func $send (param i32 i32 i32 i64 i32)))
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
                call $send
              )
          )"#;

        let mut runner = new_test_runner();

        runner
            .init_program(InitializeProgramInfo {
                new_program_id: 1.into(),
                source_id: 1001.into(),
                code: parse_wat(wat),
                message: ExtMessage {
                    id: 1000001.into(),
                    payload: "init".as_bytes().to_vec(),
                    gas_limit: u64::MAX,
                    value: 0,
                },
            })
            .expect("failed to init program");

        runner.run_next(u64::MAX);

        assert_eq!(
            runner
                .message_queue
                .dequeue()
                .map(|m| (m.payload().to_vec(), m.source(), m.dest())),
            Some((b"ok".to_vec(), 1.into(), 1.into()))
        );

        runner.queue_message(MessageDispatch {
            source_id: 1001.into(),
            destination_id: 1.into(),
            data: ExtMessage {
                id: 1000002.into(),
                payload: b"test".to_vec(),
                gas_limit: u64::MAX,
                value: 0,
            },
        });

        runner.run_next(u64::MAX);

        assert_eq!(
            runner
                .message_queue
                .dequeue()
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
            (import "env" "gr_send"  (func $send (param i32 i32 i32 i64 i32)))
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
              (call $send (i32.const 12) (i32.const 0) (i32.const 2) (i64.const 10000000000) (i32.const 32768))
            )
          )"#;

        let mut runner = Runner::new(&Config::default(), InMemoryStorage::default());

        runner
            .init_program(InitializeProgramInfo {
                new_program_id: 1.into(),
                source_id: 1001.into(),
                code: parse_wat(wat),
                message: ExtMessage {
                    id: 1000001.into(),
                    payload: vec![],
                    gas_limit: u64::MAX,
                    value: 0,
                },
            })
            .expect("failed to init program");

        runner.run_next(u64::MAX);

        assert_eq!(
            runner
                .message_queue
                .dequeue()
                .map(|m| (m.payload().to_vec(), m.source(), m.dest())),
            Some((b"ok".to_vec(), 1.into(), 1.into()))
        );

        // send page num to be freed
        runner.queue_message(MessageDispatch {
            source_id: 1001.into(),
            destination_id: 1.into(),
            data: ExtMessage {
                id: 1000002.into(),
                payload: vec![256u32 as _],
                gas_limit: u64::MAX,
                value: 0,
            },
        });

        runner.run_next(u64::MAX);

        assert_eq!(
            runner
                .message_queue
                .dequeue()
                .map(|m| (m.payload().to_vec(), m.source(), m.dest())),
            Some((vec![256u32 as _].into(), 1.into(), 1.into()))
        );
    }

    #[test]
    fn gas_transfer() {
        // Charge 100_000 of gas.
        let wat = r#"
        (module
            (import "env" "gr_charge" (func $charge (param i64)))
            (import "env" "memory" (memory 1))
            (export "handle" (func $handle))
            (export "init" (func $init))
            (func $handle
                i64.const 100000
                call $charge
            )
            (func $init)
        )"#;

        let mut runner = Runner::new(&Config::default(), InMemoryStorage::default());

        let gas_limit = 1000_000;
        let caller_id = 0.into();
        let program_id = 1.into();

        runner
            .init_program(InitializeProgramInfo {
                new_program_id: 1.into(),
                source_id: 1001.into(),
                code: parse_wat(wat),
                message: ExtMessage {
                    id: 1000001.into(),
                    payload: "init".as_bytes().to_vec(),
                    gas_limit: u64::MAX,
                    value: 0,
                },
            })
            .expect("failed to init program");

        runner.queue_message(MessageDispatch {
            source_id: caller_id,
            destination_id: 1.into(),
            data: ExtMessage {
                id: 1000002.into(),
                payload: vec![0],
                gas_limit,
                value: 0,
            },
        });

        let result = runner.run_next(u64::MAX);
        assert_eq!(result.gas_spent.len(), 1);
        assert_eq!(result.gas_left.len(), 1);
        assert_eq!(result.gas_requests.len(), 1);

        assert_eq!(result.gas_left[0].0, caller_id);
        assert!(result.gas_left[0].1 < gas_limit - 100_000);
        assert_eq!(result.gas_spent[0].0, caller_id);
        assert!(result.gas_spent[0].1 > 0 && result.gas_spent[0].1 < 100_000);

        assert_eq!(result.gas_requests[0].0, caller_id);
        assert_eq!(result.gas_requests[0].1, program_id);
        assert_eq!(result.gas_requests[0].2, 100_000);
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

        let mut runner = Runner::new(&Config::default(), InMemoryStorage::default());

        let source_id = 1001.into();
        let dest_id = 1.into();
        let gas_limit = 1_000_000;
        let msg_id: MessageId = 1000001.into();

        runner
            .init_program(InitializeProgramInfo {
                new_program_id: dest_id,
                source_id,
                code: parse_wat(wat),
                message: ExtMessage {
                    id: 1000001.into(),
                    payload: "init".as_bytes().to_vec(),
                    gas_limit,
                    value: 0,
                },
            })
            .expect("failed to init program");

        let payload = b"Test Wait";

        runner.queue_message(MessageDispatch {
            source_id,
            destination_id: 1.into(),
            data: ExtMessage {
                id: msg_id,
                payload: payload.to_vec(),
                gas_limit: 1_000_000,
                value: 0,
            },
        });

        let _result = runner.run_next(u64::MAX);

        let InMemoryStorage {
            message_queue: _,
            program_storage: _,
            wait_list,
        } = runner.complete();
        let mut wait_list: MessageMap = wait_list.into();

        assert!(wait_list.contains_key(&msg_id));
        let msg = wait_list.remove(&msg_id).unwrap();

        assert_eq!(msg.source, source_id);
        assert_eq!(msg.dest, dest_id);
        assert_eq!(msg.payload(), payload);
        assert!(msg.gas_limit < gas_limit);
    }

    #[test]
    fn gas_available() {
        // Charge 100_000 of gas.
        let wat = r#"
        (module
            (import "env" "gr_send"  (func $send (param i32 i32 i32 i64 i32)))
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
                (call $send (i32.const 12) (i32.const 18) (i32.const 8) (i64.const 1000) (i32.const 32768))
            )
            (func $init)
        )"#;

        let mut runner = Runner::new(&Config::default(), InMemoryStorage::default());

        let gas_limit = 1000_000;
        let caller_id = 1001.into();

        runner
            .init_program(InitializeProgramInfo {
                new_program_id: 1.into(),
                source_id: 1001.into(),
                code: parse_wat(wat),
                message: ExtMessage {
                    id: 1000001.into(),
                    payload: "init".as_bytes().to_vec(),
                    gas_limit: u64::MAX,
                    value: 0,
                },
            })
            .expect("failed to init program");

        runner.queue_message(MessageDispatch {
            source_id: caller_id,
            destination_id: 1.into(),
            data: ExtMessage {
                id: 1000001.into(),
                payload: vec![],
                gas_limit: 1_000_000,
                value: 0,
            },
        });

        let result = runner.run_next(u64::MAX);
        assert_eq!(result.gas_spent.len(), 1);
        assert_eq!(result.gas_left.len(), 1);

        assert_eq!(result.gas_left[0].0, caller_id);
        assert!(result.gas_left[0].1 < gas_limit);

        let (gas_available, _, _) = runner
            .message_queue
            .dequeue()
            .map(|m| (m.payload().to_vec(), m.source(), m.dest()))
            .unwrap();

        use core::convert::TryInto;

        let gas_available = gas_available
            .as_slice()
            .try_into()
            .expect("slice with incorrect length");

        assert!(u64::from_le_bytes(gas_available) > result.gas_left[0].1);
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

        let mut runner = Runner::new(&Config::default(), InMemoryStorage::default());

        let gas_limit = 1000_000;
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

        runner.queue_message(MessageDispatch {
            source_id: caller_id,
            destination_id: 1.into(),
            data: ExtMessage {
                id: 1000001.into(),
                payload: vec![],
                gas_limit: 1_000_000,
                value: 0,
            },
        });

        // Charge 1000 of gas for initial memory.
        assert_eq!(init_result.gas_spent, runner.init_cost() * 1);

        let result = runner.run_next(u64::MAX);

        assert_eq!(
            result.gas_spent[0].1,
            runner.alloc_cost() * 1 + runner.load_page_cost() * 1 + 3000
        );

        runner.complete();
    }
}
