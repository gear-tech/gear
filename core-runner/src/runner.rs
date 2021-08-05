//! Module for running programs.

use alloc::boxed::Box;
use alloc::collections::BTreeSet;
use alloc::string::String;
use alloc::vec::Vec;
use codec::{Decode, Encode};

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
    /// Totl pages count.
    pub max_pages: PageNumber,
}

const EXIT_CODE_PANIC: i32 = 1;

impl Default for Config {
    fn default() -> Self {
        Self {
            max_pages: MAX_PAGES.into(),
        }
    }
}

type GasRequest = (ProgramId, ProgramId, u64);

/// Result of one or more message handling.
#[derive(Debug, Default, Clone)]
pub struct RunNextResult {
    /// How many messages were handled
    pub handled: u32,
    /// How many messages ended as traps
    pub traps: u32,
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
            traps: 1,
            ..Default::default()
        }
    }

    /// Add request all the gas to be reserved for the destination
    pub(crate) fn refund(gas_request: GasRequest) -> Self {
        RunNextResult {
            handled: 1,
            traps: 1,
            gas_requests: vec![gas_request],
            ..Default::default()
        }
    }

    /// Accrue one run of the message hadling.
    pub fn accrue(&mut self, caller_id: ProgramId, program_id: ProgramId, result: RunResult) {
        self.handled += 1;
        if result.was_trap {
            self.traps += 1;
        }
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
    pub fn from_single(caller_id: ProgramId, program_id: ProgramId, run_result: RunResult) -> Self {
        let mut result = Self::empty();
        result.accrue(caller_id, program_id, run_result);
        result
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
pub struct Runner<MQ: MessageQueue, PS: ProgramStorage> {
    pub(crate) program_storage: PS,
    pub(crate) message_queue: MQ,
    pub(crate) config: Config,
    env: Environment<Ext>,
}

impl<MQ: MessageQueue, PS: ProgramStorage> Runner<MQ, PS> {
    /// New runner instance.
    ///
    /// Provide configuration, storage.
    pub fn new(config: &Config, storage: Storage<MQ, PS>) -> Self {
        let env = Environment::new();

        let Storage {
            message_queue,
            program_storage,
        } = storage;

        Self {
            program_storage,
            message_queue,
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

            let run_result = run(
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

            if run_result.was_trap {
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

            let result =
                RunNextResult::from_single(next_message_source, next_message_dest, run_result);

            self.message_queue
                .queue_many(context.message_buf.drain(..).collect());
            self.program_storage.set(program);

            result
        }
    }

    /// Drop this runner.
    ///
    /// This will return underlyign storage and memory state.
    pub fn complete(self) -> Storage<MQ, PS> {
        let Runner {
            program_storage,
            message_queue,
            ..
        } = self;

        Storage {
            message_queue,
            program_storage,
        }
    }

    /// Max pages configuratio of this runner.
    pub fn max_pages(&self) -> PageNumber {
        self.config.max_pages
    }

    fn create_context(&self, allocations: BTreeSet<PageNumber>) -> RunningContext {
        RunningContext::new(&self.config, allocations)
    }

    /// Initialize new program.
    ///
    /// This includes putting this program in the storage and dispatching
    /// initializationg message for it.
    #[allow(clippy::too_many_arguments)]
    pub fn init_program(
        &mut self,
        source: ProgramId,
        nonce: u64,
        program_id: ProgramId,
        code: Vec<u8>,
        init_msg: Vec<u8>,
        gas_limit: u64,
        value: u128,
    ) -> anyhow::Result<RunResult> {
        if let Some(mut program) = self.program_storage.get(program_id) {
            program.reset(code.to_vec());
            self.program_storage.set(program);
        } else {
            self.program_storage
                .set(Program::new(program_id, code, Default::default())?);
        }

        let mut program = self
            .program_storage
            .get(program_id)
            .expect("Added above; cannot fail");

        let allocations: BTreeSet<PageNumber> = (0..program.static_pages())
            .map(|page| page.into())
            .collect();

        let mut context = self.create_context(allocations);

        // TODO: figure out message id for initialization message
        let msg = IncomingMessage::new(
            self.next_message_id(source, nonce),
            source,
            init_msg.into(),
            gas_limit,
            value,
        );

        let res = run(
            &mut self.env,
            &mut context,
            &gas::instrument(program.code())
                .map_err(|e| anyhow::anyhow!("Error instrumenting: {:?}", e))?,
            &mut program,
            EntryPoint::Init,
            &msg,
            gas_limit,
        );

        self.message_queue
            .queue_many(context.message_buf.drain(..).collect());
        self.program_storage.set(program);

        Ok(res)
    }

    // TODO: Remove once parallel and "system origin" is ditched
    fn next_message_id(&mut self, source: ProgramId, nonce: u64) -> MessageId {
        let mut id_generator = BlakeMessageIdGenerator {
            program_id: source,
            nonce,
        };

        id_generator.next()
    }

    /// Queue message for the underlying message queue.
    pub fn queue_message(
        &mut self,
        source: ProgramId,
        nonce: u64,
        destination: ProgramId,
        payload: Vec<u8>,
        gas_limit: u64,
        value: u128,
    ) {
        let message_id = self.next_message_id(source, nonce);
        self.message_queue.queue(Message::new(
            message_id,
            source,
            destination,
            payload.into(),
            gas_limit,
            value,
        ));
    }

    /// Queue message for the underlying message queue.
    #[allow(clippy::too_many_arguments)]
    pub fn queue_reply(
        &mut self,
        source: ProgramId,
        nonce: u64,
        destination: ProgramId,
        payload: Vec<u8>,
        gas_limit: u64,
        value: u128,
        reply_to: MessageId,
        exit_code: ExitCode,
    ) {
        let message_id = self.next_message_id(source, nonce);
        self.message_queue.queue(Message::new_reply(
            message_id,
            source,
            destination,
            payload.into(),
            gas_limit,
            value,
            reply_to,
            exit_code,
        ));
    }
}

#[derive(Clone, Copy, Debug)]
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

static MAX_PAGES: u32 = 16384;

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

    fn push_message(&mut self, msg: Message) {
        self.message_buf.push(msg)
    }
}

/// The result of running some program.
#[derive(Clone, Debug, Default)]
pub struct RunResult {
    /// Messages that were generated during the run.
    pub messages: Vec<OutgoingMessage>,
    /// Reply that was received during the run.
    pub reply: Option<ReplyMessage>,
    /// Gas that was left.
    pub gas_left: u64,
    /// Gas that was spent.
    pub gas_spent: u64,
    /// Gas requested to be transferred.
    pub gas_requested: u64,
    /// Run result was a trap
    pub was_trap: bool,
}

struct Ext {
    memory_context: MemoryContext,
    messages: MessageContext<BlakeMessageIdGenerator>,
    gas_counter: Box<dyn GasCounter>,
    gas_requested: u64,
}

impl EnvExt for Ext {
    fn alloc(&mut self, pages: PageNumber) -> Result<PageNumber, &'static str> {
        self.memory_context
            .alloc(pages)
            .map_err(|_e| "Allocation error")
    }

    fn send(&mut self, msg: OutgoingPacket) -> Result<(), &'static str> {
        self.messages.send(msg).map_err(|_e| "Message send error")
    }

    fn send_init(&self, msg: OutgoingPacket) -> Result<usize, &'static str> {
        self.messages
            .send_init(msg)
            .map_err(|_e| "Message init error")
    }

    fn send_push(&self, handle: usize, buffer: &[u8]) -> Result<(), &'static str> {
        self.messages
            .send_push(handle, buffer)
            .map_err(|_e| "Payload push error")
    }

    fn reply_push(&self, buffer: &[u8]) -> Result<(), &'static str> {
        self.messages
            .reply_push(buffer)
            .map_err(|_e| "Reply payload push error")
    }

    fn send_commit(&self, handle: usize) -> Result<(), &'static str> {
        self.messages
            .send_commit(handle)
            .map_err(|_e| "Message commit error")
    }

    fn reply(&mut self, msg: ReplyPacket) -> Result<(), &'static str> {
        self.messages.reply(msg).map_err(|_e| "Reply error")
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
        self.memory_context.free(ptr).map_err(|_e| "Free error")
    }

    fn debug(&mut self, data: &str) -> Result<(), &'static str> {
        log::debug!("DEBUG: {}", data);
        Ok(())
    }

    fn set_mem(&mut self, ptr: usize, val: &[u8]) {
        self.memory_context
            .memory()
            .write(ptr, val)
            .expect("Memory out of bounds.");
    }

    fn get_mem(&mut self, ptr: usize, buffer: &mut [u8]) {
        self.memory_context.memory().read(ptr, buffer);
    }

    fn msg(&mut self) -> &[u8] {
        self.messages.current().payload()
    }

    fn gas(&mut self, val: u32) -> Result<(), &'static str> {
        if self.gas_counter.charge(val as u64) == ChargeResult::Enough {
            Ok(())
        } else {
            Err("Gas limit exceeded")
        }
    }

    fn charge(&mut self, gas: u64) -> Result<(), &'static str> {
        if self.gas_counter.charge(gas) == ChargeResult::Enough {
            self.gas_requested += gas;
            Ok(())
        } else {
            Err("Gas limit exceeded")
        }
    }

    fn value(&mut self) -> u128 {
        self.messages.current().value()
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
    let gas_counter = Box::new(GasCounterLimited(gas_limit)) as Box<dyn GasCounter>;

    let id_generator = BlakeMessageIdGenerator {
        program_id: program.id(),
        nonce: program.message_nonce(),
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
    };

    let (res, mut ext) = env.setup_and_run(
        ext,
        binary,
        program.get_pages(),
        &memory,
        entry_point.into(),
    );

    let was_trap = res.is_err();
    let _res = res.unwrap_or_default();

    // get allocated pages
    for page in ext.memory_context.allocations().clone() {
        let mut buf = vec![0u8; PageNumber::size()];
        ext.get_mem(page.offset(), &mut buf);
        program.set_page(page, &buf);
    }

    let mut messages = vec![];

    program.set_message_nonce(ext.messages.nonce());
    let (outgoing, reply) = ext.messages.drain();

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

    let gas_left = ext.gas_counter.left();
    let gas_requested = ext.gas_requested;
    let gas_spent = gas_limit - gas_left - gas_requested;

    RunResult {
        messages,
        reply,
        gas_left,
        gas_spent,
        gas_requested,
        was_trap,
    }
}

#[cfg(test)]
mod tests {
    extern crate wabt;
    use super::*;
    use env_logger::Env;
    use gear_core::storage::{InMemoryMessageQueue, InMemoryProgramStorage, InMemoryStorage};

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

    fn new_test_runner() -> Runner<InMemoryMessageQueue, InMemoryProgramStorage> {
        Runner::new(
            &Config::default(),
            gear_core::storage::new_in_memory(Default::default(), Default::default()),
        )
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
            .init_program(
                1001.into(),
                0,
                1.into(),
                parse_wat(wat),
                Vec::new(),
                u64::max_value(),
                0,
            )
            .expect("failed to init program");

        runner.queue_message(1001.into(), 1, 1.into(), Vec::new(), u64::max_value(), 0);

        assert_eq!(runner.run_next(u64::MAX).traps, 1);

        let msg = vec![
            1, 3, 5, 7, 9, 11, 13, 15, 17, 19, 21, 23, 25, 27, 29, 31, 2, 4, 6, 8, 10, 12, 14, 16,
            18, 20, 22, 24, 26, 28, 30, 32,
        ];

        runner.queue_reply(
            1001.into(),
            1,
            1.into(),
            Vec::new(),
            u64::max_value(),
            0,
            MessageId::from_slice(&msg),
            0,
        );

        assert_eq!(runner.run_next(u64::MAX).traps, 1); // this is handling of automatic reply when first message was trapped; it will also fail
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
                i64.const 18446744073709551615
                i32.const 0
                call $send
              )
          )"#;

        let mut runner = new_test_runner();

        runner
            .init_program(
                1001.into(),
                0,
                1.into(),
                parse_wat(wat),
                "init".as_bytes().to_vec(),
                u64::max_value(),
                0,
            )
            .expect("failed to init program");

        runner.run_next(u64::MAX);

        assert_eq!(
            runner
                .message_queue
                .dequeue()
                .map(|m| (m.payload().to_vec(), m.source(), m.dest())),
            Some((b"ok".to_vec(), 1.into(), 1.into()))
        );

        runner.queue_message(
            1001.into(),
            0,
            1.into(),
            "test".as_bytes().to_vec(),
            u64::max_value(),
            0,
        );

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
              i64.const 18446744073709551615
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
              (call $send (i32.const 12) (i32.const 0) (i32.const 2) (i64.const 18446744073709551615) (i32.const 32768))
            )
          )"#;

        let mut runner = Runner::new(
            &Config::default(),
            gear_core::storage::new_in_memory(Default::default(), Default::default()),
        );

        runner
            .init_program(
                1001.into(),
                0,
                1.into(),
                parse_wat(wat),
                vec![],
                u64::max_value(),
                0,
            )
            .expect("Failed to init program");

        runner.run_next(u64::MAX);

        assert_eq!(
            runner
                .message_queue
                .dequeue()
                .map(|m| (m.payload().to_vec(), m.source(), m.dest())),
            Some((b"ok".to_vec(), 1.into(), 1.into()))
        );

        // send page num to be freed
        runner.queue_message(
            1001.into(),
            1,
            1.into(),
            vec![256u32 as _],
            u64::max_value(),
            0,
        );

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
          (import "env" "gr_charge"  (func $charge (param i64)))
          (import "env" "memory" (memory 1))
          (export "handle" (func $handle))
          (export "init" (func $init))
          (func $handle
            i64.const 100000
            call $charge
          )
          (func $init)
        )"#;

        let mut runner = Runner::new(
            &Config::default(),
            gear_core::storage::new_in_memory(Default::default(), Default::default()),
        );

        let gas_limit = 1000_000;
        let caller_id = 0.into();
        let program_id = 1.into();
        let _ = runner
            .init_program(
                1001.into(),
                0,
                program_id,
                parse_wat(wat),
                "init".as_bytes().to_vec(),
                gas_limit,
                0,
            )
            .expect("failed to init `gas_transfer` program");

        runner.queue_message(caller_id, 1, 1.into(), vec![0], gas_limit, 0);

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
}
