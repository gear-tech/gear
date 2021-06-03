//! Module for running programs.

use wasmtime::{Module, Memory as WasmMemory};
use codec::{Encode, Decode};
use anyhow::Result;

use crate::{
    env::{Environment, Ext as EnvExt, PageAction},
    memory::{Allocations, MemoryContext, PageNumber},
    message::{IncomingMessage, Message, MessageContext, OutgoingMessage},
    program::{Program, ProgramId},
    storage::{AllocationStorage, MessageQueue, ProgramStorage, Storage},
    gas::{self, GasCounter, GasCounterLimited, GasCounterUnlimited, ChargeResult},
};

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
        Self { static_pages: BASIC_PAGES.into(), max_pages: MAX_PAGES.into() }
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
}

impl RunNextResult {
    /// Result that notes that some log message had been handled, otherwise empty.
    pub(crate) fn log() -> Self {
        RunNextResult { handled: 1, ..Default::default() }
    }

    /// Accrue one run of the message hadling
    pub fn accrue(&mut self, program_id: ProgramId, result: RunResult) {
        self.handled += 1;
        self.touched.extend(result.touched.into_iter());
        self.gas_left.push(
            (program_id, result.gas_left)
        );
        self.gas_spent.push(
            (program_id, result.gas_spent)
        );
    }

    /// Empty run result.
    pub fn empty() -> Self {
        RunNextResult::default()
    }

    /// From one single run.
    pub fn from_single(program_id: ProgramId, run_result: RunResult) -> Self {
        let mut result = Self::empty();
        result.accrue(program_id, run_result);
        result
    }
}

/// Runner instance.
///
/// This instance allows to handle multiple messages using underlying allocation, message and program
/// storage.
pub struct Runner<AS: AllocationStorage + 'static, MQ: MessageQueue, PS: ProgramStorage> {
    pub(crate) program_storage: PS,
    pub(crate) message_queue: MQ,
    pub(crate) memory: WasmMemory,
    pub(crate) allocations: Allocations<AS>,
    pub(crate) config: Config,
    env: Environment<Ext<AS>>,
}

impl<AS: AllocationStorage + 'static, MQ: MessageQueue, PS: ProgramStorage> Runner<AS, MQ, PS> {
    /// New runner instance.
    ///
    /// Provide configuration, storage and memory state.
    pub fn new(
        config: &Config,
        storage: Storage<AS, MQ, PS>,
        persistent_memory: &[u8],
    ) -> Self {
        // memory need to be at least static_pages + persistent_memory length (in pages)
        let persistent_pages = persistent_memory.len() / BASIC_PAGE_SIZE;
        let total_pages = config.static_pages.raw() + persistent_pages as u32;

        let env = Environment::new();
        let memory = env.create_memory(total_pages);

        let persistent_region_start = config.static_pages.raw() as usize * BASIC_PAGE_SIZE;
        let persistent_region_end = persistent_region_start + persistent_memory.len();

        unsafe {
            memory
                .data_unchecked_mut()[persistent_region_start..persistent_region_end]
                .copy_from_slice(persistent_memory);
        }

        let Storage { allocation_storage, message_queue, program_storage } = storage;

        Self {
            program_storage,
            message_queue,
            memory,
            allocations: Allocations::new(allocation_storage),
            config: config.clone(),
            env,
        }
    }

    /// Run handlig next message in the queue.
    ///
    /// Runner will return actual number of messages that was handled.
    /// Messages with no destination won't be handled.
    pub fn run_next(&mut self) -> Result<RunNextResult> {
        let next_message = match self.message_queue.dequeue() {
            Some(msg) => msg,
            None => { return Ok(RunNextResult::empty()); }
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
                .expect("Program not found");

            let gas_limit = next_message.gas_limit();

            let module = Module::new(
                self.env.engine(),
                &gas::instrument(program.code())
                    .map_err(|e| anyhow::anyhow!("Error instrumenting: {:?}", e))?,
            )?;

            let result = RunNextResult::from_single(
                next_message.source(),
                run(
                    &mut self.env,
                    &mut context,
                    module,
                    &mut program,
                    EntryPoint::Handle,
                    &next_message.into(),
                    if let Some(gas_limit) = gas_limit { GasLimit::Limited(gas_limit) } else { GasLimit::Unlimited },
                )?
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
        let persistent_memory = {
            let non_static_region_start = self.static_pages().raw() as usize * BASIC_PAGE_SIZE;
            unsafe { &self.memory.data_unchecked()[non_static_region_start..] }.to_vec()
        };

        let Runner { program_storage, message_queue, allocations, .. } = self;

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
        RunningContext::new(
            &self.config,
            self.memory.clone(),
            self.allocations.clone(),
        )
    }

    /// Initialize new program.
    ///
    /// This includes putting this program in the storage and dispatching
    /// initializationg message for it.
    pub fn init_program(
        &mut self,
        program_id: ProgramId,
        code: Vec<u8>,
        init_msg: Vec<u8>,
        gas_limit: u64,
        value: u128,
    ) -> Result<RunResult> {
        if let Some(mut program) = self.program_storage.get(program_id) {
            program.set_code(code.to_vec());
            program.clear_static();
            self.program_storage.set(program);
        } else {
            self.program_storage.set(Program::new(program_id, code, vec![]));
        }

        let mut context = self.create_context();
        let mut program = self
            .program_storage
            .get(program_id)
            .expect("Added above; cannot fail");
        let msg = IncomingMessage::new_system(init_msg.into(), Some(gas_limit), value);

        let module = Module::new(
            self.env.engine(),
            &gas::instrument(program.code())
                .map_err(|e| anyhow::anyhow!("Error instrumenting: {:?}", e))?,
        )?;

        let res = run(
            &mut self.env,
            &mut context,
            module,
            &mut program,
            EntryPoint::Init,
            &msg,
            GasLimit::Limited(gas_limit),
        )?;

        self.message_queue
            .queue_many(context.message_buf.drain(..).collect());
        self.program_storage.set(program);

        Ok(res)
    }

    /// Queue message for the underlying message queue.
    pub fn queue_message(
        &mut self,
        destination: ProgramId,
        payload: Vec<u8>,
        gas_limit: Option<u64>,
        value: u128,
    ) {
        self.message_queue
            .queue(Message::new_system(destination, payload.into(), gas_limit, value))
    }
}

#[derive(Clone, Copy, Debug)]
enum EntryPoint {
    Handle,
    Init,
}

impl From<EntryPoint> for &'static str {
    fn from(entry_point: EntryPoint) -> &'static str {
        match entry_point {
            EntryPoint::Handle => "handle",
            EntryPoint::Init => "init",
        }
    }
}

static BASIC_PAGES: u32 = 256;
static BASIC_PAGE_SIZE: usize = 65536;
static MAX_PAGES: u32 = 16384;

struct RunningContext<AS: AllocationStorage> {
    config: Config,
    memory: WasmMemory,
    allocations: Allocations<AS>,
    message_buf: Vec<Message>,
}

impl<AS: AllocationStorage> RunningContext<AS> {
    fn new(
        config: &Config,
        memory: WasmMemory,
        allocations: Allocations<AS>,
    ) -> Self {
        Self {
            config: config.clone(),
            message_buf: vec![],
            memory,
            allocations,
        }
    }

    fn wasmtime_memory(&self) -> wasmtime::Memory {
        self.memory.clone()
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
    /// Gas that was left.
    pub gas_left: u64,
    /// Gas that was spent.
    pub gas_spent: u64,
}


struct Ext<AS: AllocationStorage + 'static> {
    memory_context: MemoryContext<AS>,
    messages: MessageContext,
    gas_counter: Box<dyn GasCounter>,
}

impl<AS: AllocationStorage + 'static> EnvExt for Ext<AS> {
    fn alloc(&mut self, pages: PageNumber) -> Result<PageNumber, &'static str> {
        self.memory_context.alloc(pages).map_err(|_e| "Allocation error")
    }

    fn send(&mut self, msg: OutgoingMessage) -> Result<(), &'static str> {
        self.messages.send(msg).map_err(|_e| "Message send error")
    }

    fn source(&mut self) -> Option<ProgramId> {
        self.messages.current().source()
    }

    fn free(&mut self, ptr: PageNumber) -> Result<(), &'static str> {
        self.memory_context.free(ptr).map_err(|_e| "Free error")
    }

    fn debug(&mut self, data: &str) -> Result<(), &'static str> {
        log::debug!("DEBUG: {}", data);
        Ok(())
    }

    fn set_mem(&mut self, ptr: usize, val: &[u8]) {
        unsafe {
            self
                .memory_context
                .memory()
                .data_unchecked_mut()[ptr..ptr+val.len()]
                .copy_from_slice(val);
        }
    }

    fn get_mem(&mut self, ptr: usize, len: usize) -> &[u8] {
        unsafe { &self.memory_context.memory().data_unchecked()[ptr..ptr+len] }
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
        if self.gas_counter.charge(val) == ChargeResult::Enough {
            Ok(())
        } else {
            Err("Gas limit exceeded")
        }
    }

    fn value(&mut self) -> u128 {
        self.messages.current().value()
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum GasLimit {
    Limited(u64),
    Unlimited,
}

fn run<AS: AllocationStorage + 'static>(
    env: &mut Environment<Ext<AS>>,
    context: &mut RunningContext<AS>,
    module: Module,
    program: &mut Program,
    entry_point: EntryPoint,
    message: &IncomingMessage,
    gas_limit: GasLimit,
) -> Result<RunResult> {

    let gas_counter = match gas_limit {
        GasLimit::Limited(val) => Box::new(GasCounterLimited(val)) as Box<dyn GasCounter>,
        GasLimit::Unlimited => Box::new(GasCounterUnlimited) as Box<dyn GasCounter>,
    };

    let ext = Ext {
        memory_context: MemoryContext::new(
            program.id(),
            Box::new(context.wasmtime_memory()),
            context.allocations.clone(),
            context.static_pages(),
            context.max_pages(),
        ),
        messages: MessageContext::new(message.clone()),
        gas_counter,
    };

    // Set static pages from saved program state.

    let static_area = program.static_pages().to_vec();

    let (res, mut ext, touched) = env.setup_and_run(
        ext,
        module,
        static_area,
        context.wasmtime_memory(),
        move |instance| {
            instance
                .get_func(entry_point.into())
                .ok_or(
                    anyhow::format_err!("failed to find `{}` function export", Into::<&'static str>::into(entry_point))
                )
                .and_then(|entry_func| entry_func.call(&[]))
                .map(|_| ())
        },
    );

    res.map(move |_| {
        *program.static_pages_mut() = ext.get_mem(0, context.static_pages().raw() as usize * BASIC_PAGE_SIZE).to_vec();

        let mut messages = vec![];
        for outgoing_msg in ext.messages.drain() {
            messages.push(outgoing_msg.clone());
            context.push_message(outgoing_msg.into_message(program.id()));
        }

        let gas_left = ext.gas_counter.left();
        let gas_spent = match gas_limit {
            GasLimit::Limited(total) => total - gas_left,
            GasLimit::Unlimited => 0,
        };

        RunResult {
            touched,
            messages,
            gas_left,
            gas_spent,
        }
    })
}

#[cfg(test)]
mod tests {
    extern crate wabt;
    use super::*;

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
    fn runner_simple() {
        // Sends "ok" on init, then sends back the message it retrieved from the handle
        let wat = r#"
        (module
            (import "env" "read"  (func $read (param i32 i32 i32)))
            (import "env" "send"  (func $send (param i32 i32 i32 i64 i32)))
            (import "env" "size"  (func $size (result i32)))
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

        let mut runner = Runner::new(
            &Config::default(),
            crate::storage::new_in_memory(
                Default::default(),
                Default::default(),
                Default::default(),
            ),
            &[],
        );

        runner
            .init_program(1.into(), parse_wat(wat), "init".as_bytes().to_vec(), crate::gas::max_gas(), 0)
            .expect("failed to init program");

        runner.run_next().expect("Failed to process next message");

        assert_eq!(
            runner.message_queue.dequeue(),
            Some(Message {
                source: 1.into(),
                dest: 1.into(),
                payload: "ok".as_bytes().to_vec().into(),
                gas_limit: Some(0),
                value: 0,
            })
        );

        runner.queue_message(1.into(), "test".as_bytes().to_vec(), None, 0);

        runner.run_next().expect("Failed to process next message");

        assert_eq!(
            runner.message_queue.dequeue(),
            Some(Message {
                source: 1.into(),
                dest: 1.into(),
                payload: "test".as_bytes().to_vec().into(),
                gas_limit: Some(0),
                value: 0,
            })
        );
    }

    #[test]
    fn runner_allocations() {
        // alloc 1 page in init
        // free page num from message in handle and send it back
        let wat = r#"
        (module
            (import "env" "read"  (func $read (param i32 i32 i32)))
            (import "env" "send"  (func $send (param i32 i32 i32 i64 i32)))
            (import "env" "size"  (func $size (result i32)))
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
            crate::storage::new_in_memory(
                Default::default(),
                Default::default(),
                Default::default(),
            ),
            &[],
        );

        runner
            .init_program(1.into(), parse_wat(wat), vec![], crate::gas::max_gas(), 0)
            .expect("Failed to init program");

        // check if page belongs to the program
        assert_eq!(runner.allocations.get(256.into()), Some(ProgramId::from(1)));

        runner.run_next().expect("Failed to process next message");

        assert_eq!(
            runner.message_queue.dequeue(),
            Some(Message {
                source: 1.into(),
                dest: 1.into(),
                payload: "ok".as_bytes().to_vec().into(),
                gas_limit: Some(18446744073709551615),
                value: 0,
            })
        );

        // send page num to be freed
        runner.queue_message(1.into(), vec![256u32 as _], None, 0);

        runner.run_next().expect("Failed to process next message");

        assert_eq!(
            runner.message_queue.dequeue(),
            Some(Message {
                source: 1.into(),
                dest: 1.into(),
                payload: vec![256u32 as _].into(),
                gas_limit: Some(18446744073709551615),
                value: 0,
            })
        );

        // page is now deallocated
        assert_eq!(runner.allocations.get(256.into()), None);
    }

    #[test]
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
        let wat_w= r#"
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
            crate::storage::new_in_memory(
                Default::default(),
                Default::default(),
                Default::default(),
            ),
            &[],
        );

        let result = runner.init_program(1.into(), parse_wat(wat_r), "init".as_bytes().to_vec(), crate::gas::max_gas(), 0)
            .expect("failed to init program 1");

        assert_eq!(result.touched[0], (1.into(), PageAction::Read));

        let result = runner.init_program(2.into(), parse_wat(wat_w), "init".as_bytes().to_vec(), crate::gas::max_gas(), 0)
            .expect("failed to init program 2");

        assert_eq!(result.touched[0], (2.into(), PageAction::Write));

        let (_, persistent_memory) = runner.complete();

        assert_eq!(persistent_memory[0], 0);
    }
}
