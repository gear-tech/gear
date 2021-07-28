//! Module for running programs.

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use anyhow::{Error, Result};
use codec::{Decode, Encode};

use gear_core::{
    env::{Ext as EnvExt, PageAction},
    gas::{self, ChargeResult, GasCounter, GasCounterLimited},
    memory::{Allocations, Memory, MemoryContext, PageNumber},
    message::{
        IncomingMessage, Message, MessageContext, MessageId, MessageIdGenerator, OutgoingMessage,
        OutgoingPacket, ReplyMessage, ReplyPacket,
    },
    program::{Program, ProgramId},
    storage::{AllocationStorage, MessageQueue, ProgramStorage, Storage},
};

use gear_core_backend::Environment;

/// Runner configuration.
#[derive(Clone, Debug, Decode, Encode)]
pub struct Config {
    /// Number of static pages.
    pub static_pages: PageNumber,
    /// Totl pages count.
    pub max_pages: PageNumber,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            static_pages: BASIC_PAGES.into(),
            max_pages: MAX_PAGES.into(),
        }
    }
}

/// Result of one or more message handling.
#[derive(Debug, Default, Clone)]
pub struct RunNextResult {
    /// How many messages were handled
    pub handled: u32,
    /// Pages that were touched during the run.
    pub touched: Vec<(PageNumber, PageAction)>,
    /// Gas that was left.
    pub gas_left: Vec<(ProgramId, u64)>,
    /// Gas that was spent.
    pub gas_spent: Vec<(ProgramId, u64)>,
    /// Gas transfer requests.
    pub gas_requests: Vec<(ProgramId, ProgramId, u64)>,
}

impl RunNextResult {
    /// Result that notes that some log message had been handled, otherwise empty.
    pub(crate) fn log() -> Self {
        RunNextResult {
            handled: 1,
            ..Default::default()
        }
    }

    /// Accrue one run of the message hadling.
    pub fn accrue(&mut self, caller_id: ProgramId, program_id: ProgramId, result: RunResult) {
        self.handled += 1;
        self.touched.extend(result.touched.into_iter());
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

        MessageId::from_slice(&blake2_rfc::blake2b::blake2b(32, &[], &data).as_bytes())
    }

    fn current(&self) -> u64 {
        self.nonce
    }
}

/// Runner instance.
///
/// This instance allows to handle multiple messages using underlying allocation, message and program
/// storage.
pub struct Runner<AS: AllocationStorage + 'static, MQ: MessageQueue, PS: ProgramStorage> {
    pub(crate) program_storage: PS,
    pub(crate) message_queue: MQ,
    pub(crate) memory: Box<dyn Memory>,
    pub(crate) allocations: Allocations<AS>,
    pub(crate) config: Config,
    env: Environment<Ext<AS>>,
}

impl<AS: AllocationStorage + 'static, MQ: MessageQueue, PS: ProgramStorage> Runner<AS, MQ, PS> {
    /// New runner instance.
    ///
    /// Provide configuration, storage and memory state.
    pub fn new(config: &Config, storage: Storage<AS, MQ, PS>, persistent_memory: &[u8]) -> Self {
        // memory need to be at least static_pages + persistent_memory length (in pages)
        let persistent_pages = persistent_memory.len() / PageNumber::size();
        let total_pages = config.static_pages.raw() + persistent_pages as u32;

        let env = Environment::new();
        let memory = env.create_memory(total_pages);

        let persistent_region_start = config.static_pages.offset();

        memory
            .write(persistent_region_start, persistent_memory)
            .expect("Memory out of bounds.");

        let Storage {
            allocation_storage,
            message_queue,
            program_storage,
        } = storage;

        Self {
            program_storage,
            message_queue,
            memory: Box::new(memory),
            allocations: Allocations::new(allocation_storage),
            config: config.clone(),
            env,
        }
    }

    /// Run handling next message in the queue.
    ///
    /// Runner will return actual number of messages that was handled.
    /// Messages with no destination won't be handled.
    pub fn run_next(&mut self) -> Result<RunNextResult> {
        let next_message = match self.message_queue.dequeue() {
            Some(msg) => msg,
            None => {
                return Ok(RunNextResult::empty());
            }
        };

        if next_message.dest() == 0.into() {
            match String::from_utf8(next_message.payload().to_vec()) {
                Ok(s) => log::debug!("UTF-8 msg to /0: {}", s),
                Err(_) => {
                    log::debug!("msg to /0: {:?}", next_message.payload())
                }
            }
            Ok(RunNextResult::log())
        } else {
            let mut context = self.create_context();
            let mut program = self
                .program_storage
                .get(next_message.dest())
                .ok_or_else(|| Error::msg("Program not found"))?;

            let gas_limit = next_message.gas_limit();

            let result = RunNextResult::from_single(
                next_message.source(),
                next_message.dest(),
                run(
                    &mut self.env,
                    &mut context,
                    &gas::instrument(program.code())
                        .map_err(|e| anyhow::anyhow!("Error instrumenting: {:?}", e))?,
                    &mut program,
                    if next_message.reply().is_some() {
                        EntryPoint::HandleReply
                    } else {
                        EntryPoint::Handle
                    },
                    &next_message.into(),
                    gas_limit,
                )?,
            );

            self.message_queue
                .queue_many(context.message_buf.drain(..).collect());
            self.program_storage.set(program);

            Ok(result)
        }
    }

    /// Drop this runner.
    ///
    /// This will return underlyign storage and memory state.
    pub fn complete(self) -> (Storage<AS, MQ, PS>, Vec<u8>) {
        let mut persistent_memory =
            vec![0u8; self.memory.data_size() - self.static_pages().offset()];
        self.memory
            .read(self.static_pages().offset(), &mut persistent_memory);

        let Runner {
            program_storage,
            message_queue,
            allocations,
            ..
        } = self;

        let allocation_storage = match allocations.drain() {
            Ok(v) => v,
            Err(e) => {
                panic!("Panic finalizing allocations: {:?}", e)
            }
        };

        (
            Storage {
                allocation_storage,
                message_queue,
                program_storage,
            },
            persistent_memory,
        )
    }

    /// Static pages configuratio of this runner.
    pub fn static_pages(&self) -> PageNumber {
        self.config.static_pages
    }

    /// Max pages configuratio of this runner.
    pub fn max_pages(&self) -> PageNumber {
        self.config.max_pages
    }

    fn create_context(&self) -> RunningContext<AS> {
        RunningContext::new(&self.config, self.memory.clone(), self.allocations.clone())
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
    ) -> Result<RunResult> {
        if let Some(mut program) = self.program_storage.get(program_id) {
            program.reset(code.to_vec());
            self.program_storage.set(program);
        } else {
            self.program_storage.set(Program::new(
                program_id,
                code,
                Default::default(),
                Some(self.static_pages().raw()),
            )?);
        }

        let mut context = self.create_context();
        let mut program = self
            .program_storage
            .get(program_id)
            .expect("Added above; cannot fail");

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
        )?;

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

static BASIC_PAGES: u32 = 256;
static MAX_PAGES: u32 = 16384;

struct RunningContext<AS: AllocationStorage> {
    config: Config,
    memory: Box<dyn Memory>,
    allocations: Allocations<AS>,
    message_buf: Vec<Message>,
}

impl<AS: AllocationStorage> RunningContext<AS> {
    fn new(config: &Config, memory: Box<dyn Memory>, allocations: Allocations<AS>) -> Self {
        Self {
            config: config.clone(),
            message_buf: vec![],
            memory,
            allocations,
        }
    }

    fn memory(&self) -> &dyn Memory {
        &*self.memory
    }

    fn static_pages(&self) -> PageNumber {
        self.config.static_pages
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
    /// Pages that were touched during the run.
    pub touched: Vec<(PageNumber, PageAction)>,
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
}

struct Ext<AS: AllocationStorage + 'static> {
    memory_context: MemoryContext<AS>,
    messages: MessageContext<BlakeMessageIdGenerator>,
    gas_counter: Box<dyn GasCounter>,
    gas_requested: u64,
}

impl<AS: AllocationStorage + 'static> EnvExt for Ext<AS> {
    fn alloc(&mut self, pages: PageNumber) -> Result<PageNumber, &'static str> {
        self.memory_context
            .alloc(pages)
            .map_err(|_e| "Allocation error")
    }

    fn send(&mut self, msg: OutgoingPacket) -> Result<(), &'static str> {
        self.messages.send(msg).map_err(|_e| "Message send error")
    }

    fn init(&self, msg: OutgoingPacket) -> Result<usize, &'static str> {
        self.messages.init(msg).map_err(|_e| "Message init error")
    }

    fn push(&self, handle: usize, buffer: &[u8]) -> Result<(), &'static str> {
        self.messages
            .push(handle, buffer)
            .map_err(|_e| "Payload push error")
    }

    fn push_reply(&self, buffer: &[u8]) -> Result<(), &'static str> {
        self.messages
            .push_reply(buffer)
            .map_err(|_e| "Reply payload push error")
    }

    fn commit(&self, handle: usize) -> Result<(), &'static str> {
        self.messages
            .commit(handle)
            .map_err(|_e| "Message commit error")
    }

    fn reply(&mut self, msg: ReplyPacket) -> Result<(), &'static str> {
        self.messages.reply(msg).map_err(|_e| "Reply error")
    }

    fn reply_to(&self) -> Option<MessageId> {
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

    fn memory_access(&self, page: PageNumber) -> PageAction {
        if let Some(id) = self.memory_context.allocations().get(page) {
            if id == self.memory_context.program_id() {
                PageAction::Write
            } else {
                PageAction::Read
            }
        } else {
            PageAction::None
        }
    }

    fn memory_lock(&self) {
        self.memory_context.memory_lock();
    }

    fn memory_unlock(&self) {
        self.memory_context.memory_unlock();
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

fn run<AS: AllocationStorage + 'static>(
    env: &mut Environment<Ext<AS>>,
    context: &mut RunningContext<AS>,
    binary: &[u8],
    program: &mut Program,
    entry_point: EntryPoint,
    message: &IncomingMessage,
    gas_limit: u64,
) -> Result<RunResult> {
    let gas_counter = Box::new(GasCounterLimited(gas_limit)) as Box<dyn GasCounter>;

    let id_generator = BlakeMessageIdGenerator {
        program_id: program.id(),
        nonce: program.message_nonce(),
    };

    let ext = Ext {
        memory_context: MemoryContext::new(
            program.id(),
            context.memory().clone(),
            context.allocations.clone(),
            context.static_pages(),
            context.max_pages(),
        ),
        messages: MessageContext::new(message.clone(), id_generator),
        gas_counter,
        gas_requested: 0,
    };

    // Set static pages from saved program state.

    let (res, mut ext, touched) = env.setup_and_run(
        ext,
        binary,
        program.get_pages(),
        context.memory(),
        entry_point.into(),
    );

    res.map(move |_| {
        // TODO: Get all pages when isolated memory is introduced.
        let mut memory_pages = vec![0u8; context.static_pages().offset()];
        ext.get_mem(0, &mut memory_pages);
        program.set_memory(&memory_pages);

        let mut messages = vec![];

        program.set_message_nonce(ext.messages.nonce());
        let (outgoing, reply) = ext.messages.drain();

        for outgoing_msg in outgoing {
            messages.push(outgoing_msg.clone());
            context.push_message(outgoing_msg.into_message(program.id()));
        }

        let gas_left = ext.gas_counter.left();
        let gas_requested = ext.gas_requested;
        let gas_spent = gas_limit - gas_left - gas_requested;

        RunResult {
            touched,
            messages,
            reply,
            gas_left,
            gas_spent,
            gas_requested,
        }
    })
}

#[cfg(test)]
mod tests {
    extern crate wabt;
    use super::*;
    use env_logger::Env;
    use gear_core::storage::{
        InMemoryAllocationStorage, InMemoryMessageQueue, InMemoryProgramStorage, InMemoryStorage,
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

    fn new_test_runner(
    ) -> Runner<InMemoryAllocationStorage, InMemoryMessageQueue, InMemoryProgramStorage> {
        Runner::new(
            &Config::default(),
            gear_core::storage::new_in_memory(
                Default::default(),
                Default::default(),
                Default::default(),
            ),
            &[],
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

        match runner.run_next() {
            Ok(_) => panic!("This should be an error that we run "),
            Err(anyhow_err) => {
                format!("{}", anyhow_err).contains("Not running in the reply context");
            }
        };

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
        );

        runner.run_next().expect("Should be ok now.");

        let (
            InMemoryStorage {
                program_storage, ..
            },
            ..,
        ) = runner.complete();

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

        runner.run_next().expect("Failed to process next message");

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

        runner.run_next().expect("Failed to process next message");

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
            gear_core::storage::new_in_memory(
                Default::default(),
                Default::default(),
                Default::default(),
            ),
            &[],
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

        // check if page belongs to the program
        assert_eq!(runner.allocations.get(256.into()), Some(ProgramId::from(1)));

        runner.run_next().expect("Failed to process next message");

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

        runner.run_next().expect("Failed to process next message");

        assert_eq!(
            runner
                .message_queue
                .dequeue()
                .map(|m| (m.payload().to_vec(), m.source(), m.dest())),
            Some((vec![256u32 as _].into(), 1.into(), 1.into()))
        );

        // page is now deallocated
        assert_eq!(runner.allocations.get(256.into()), None);
    }

    #[test]
    // TODO: fix memory access logging for macos
    #[cfg_attr(target_os = "macos", ignore)]
    fn mem_rw_access() {
        // Read in new allocatted page
        let wat_r = r#"
        (module
            (import "env" "alloc"  (func $alloc (param i32) (result i32)))
            (import "env" "memory" (memory 1))
            (export "handle" (func $handle))
            (export "init" (func $init))
            (func $handle
            )
            (func $init
                (local $alloc_pages i32)
                (local $pages_offset i32)
                (local.set $pages_offset (call $alloc (i32.const 1)))

                i32.const 0
                i32.load offset=65536

                drop
              )
          )"#;

        // Write in new allocatted page
        let wat_w = r#"
        (module
            (import "env" "alloc"  (func $alloc (param i32) (result i32)))
            (import "env" "memory" (memory 1))
            (export "handle" (func $handle))
            (export "init" (func $init))
            (func $handle
            )
            (func $init
                (local $alloc_pages i32)
                (local $pages_offset i32)
                (local.set $pages_offset (call $alloc (i32.const 1)))
                (i32.store offset=131072
                    (i32.const 0)
                    (i32.const 10)
                )
              )
          )"#;

        let mut runner = Runner::new(
            &Config {
                static_pages: 1.into(),
                max_pages: 3.into(),
            },
            gear_core::storage::new_in_memory(
                Default::default(),
                Default::default(),
                Default::default(),
            ),
            &[],
        );

        let result = runner
            .init_program(
                1001.into(),
                0,
                1.into(),
                parse_wat(wat_r),
                "init".as_bytes().to_vec(),
                u64::max_value(),
                0,
            )
            .expect("failed to init program 1");

        assert_eq!(result.touched[0], (1.into(), PageAction::Read));

        let result = runner
            .init_program(
                1001.into(),
                1,
                2.into(),
                parse_wat(wat_w),
                "init".as_bytes().to_vec(),
                u64::max_value(),
                0,
            )
            .expect("failed to init program 2");

        assert_eq!(result.touched[0], (2.into(), PageAction::Write));

        let (_, persistent_memory) = runner.complete();

        assert_eq!(persistent_memory[0], 0);
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
            gear_core::storage::new_in_memory(
                Default::default(),
                Default::default(),
                Default::default(),
            ),
            &[],
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

        let result = runner.run_next().expect("Failed to process next message");
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
