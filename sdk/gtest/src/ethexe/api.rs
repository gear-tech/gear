// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use crate::{
    GAS_ALLOWANCE, Gas, MAX_USER_GAS_LIMIT, Result, Value, default_users_list,
    error::usage_panic,
    log::BlockRunResult,
    manager::ExtManager,
    program::ProgramIdWrapper,
    state::{
        accounts::Accounts,
        bridge::BridgeBuiltinStorage,
        programs::{GTestProgram, PLACEHOLDER_MESSAGE_ID, ProgramsStorageManager},
    },
};
use gear_common::Origin;
use gear_core::{
    code::{Code, CodeMetadata, InstrumentedCode, SyscallKind},
    gas_metering::Schedule,
    ids::{ActorId, CodeId, MessageId, prelude::*},
    program::{ActiveProgram, Program as PrimaryProgram, ProgramState},
};
use parity_scale_codec::{Codec, Decode, Encode};
use path_clean::PathClean;
use std::{
    borrow::Cow,
    cell::RefCell,
    env,
    ffi::OsStr,
    fmt::Debug,
    fs,
    path::{Path, PathBuf},
};
use tracing_subscriber::EnvFilter;

fn store_original_code(manager: &mut ExtManager, code_id: CodeId, code: Vec<u8>) {
    manager.opt_binaries.insert(code_id, code);
}

thread_local! {
    /// Ethexe `System` is also a singleton within a thread.
    static SYSTEM_INITIALIZED: RefCell<bool> = const { RefCell::new(false) };
}

/// Ethexe testing environment for Gear programs.
///
/// This API is intentionally separate from top-level [`crate::System`], which
/// always keeps Vara `gtest` semantics even when the `ethexe` feature is
/// enabled.
pub struct System(pub(crate) RefCell<ExtManager>);

impl System {
    /// Create a new ethexe testing environment.
    ///
    /// # Panics
    /// Only one ethexe `System` may exist in the current thread at a time.
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        SYSTEM_INITIALIZED.with_borrow_mut(|initialized| {
            if *initialized {
                panic!("Impossible to have multiple instances of the `System`.");
            }

            super::init_lazy_pages();
            *initialized = true;

            Self(RefCell::new(ExtManager::new()))
        })
    }

    /// Init logger with "gwasm" target set to `debug` level.
    pub fn init_logger(&self) {
        self.init_logger_with_default_filter("gwasm=debug");
    }

    /// Init logger with "gwasm" and "gtest" targets set to `debug` level.
    pub fn init_verbose_logger(&self) {
        self.init_logger_with_default_filter("gwasm=debug,gtest=debug");
    }

    /// Init logger with `default_filter` as default filter.
    pub fn init_logger_with_default_filter<'a>(&self, default_filter: impl Into<Cow<'a, str>>) {
        let filter = if env::var(EnvFilter::DEFAULT_ENV).is_ok() {
            EnvFilter::from_default_env()
        } else {
            EnvFilter::new(default_filter.into())
        };
        let _ = tracing_subscriber::fmt()
            .with_env_filter(filter)
            .without_time()
            .with_thread_names(true)
            .try_init();
    }

    /// Returns amount of ethexe dispatches waiting in canonical or injected queues.
    pub fn queue_len(&self) -> usize {
        self.0.borrow().ethexe().queue_len()
    }

    /// Run next ethexe block.
    pub fn run_next_block(&self) -> BlockRunResult {
        self.run_next_block_with_allowance(GAS_ALLOWANCE)
    }

    /// Run next ethexe block with limited gas allowance.
    pub fn run_next_block_with_allowance(&self, allowance: Gas) -> BlockRunResult {
        if allowance > GAS_ALLOWANCE {
            usage_panic!(
                "Provided allowance more than allowed limit of {GAS_ALLOWANCE}. \
                Please, provide an allowance less than or equal to the limit."
            );
        }

        let mut manager = self.0.borrow_mut();
        let block_info = manager.blocks_manager.next_block();
        manager
            .ethexe_mut()
            .run_new_block(block_info.height, block_info.timestamp, allowance)
    }

    /// Run ethexe blocks until `bn`, inclusive.
    pub fn run_to_block(&self, bn: u32) -> Vec<BlockRunResult> {
        let mut manager = self.0.borrow_mut();

        let mut current_block = manager.block_height();
        if current_block > bn {
            usage_panic!("Can't run blocks until bn {bn}, as current bn is {current_block}");
        }

        let mut ret = Vec::with_capacity((bn - current_block) as usize);
        while current_block != bn {
            let block_info = manager.blocks_manager.next_block();
            let res = manager.ethexe_mut().run_new_block(
                block_info.height,
                block_info.timestamp,
                GAS_ALLOWANCE,
            );
            ret.push(res);

            current_block = manager.block_height();
        }

        ret
    }

    /// Run `amount` of ethexe blocks only with scheduled tasks.
    pub fn run_scheduled_tasks(&self, amount: u32) -> Vec<BlockRunResult> {
        let mut manager = self.0.borrow_mut();
        let block_height = manager.block_height();

        (block_height..block_height + amount)
            .map(|_| {
                let block_info = manager.blocks_manager.next_block();
                manager
                    .ethexe_mut()
                    .run_scheduled_block(block_info.height, block_info.timestamp)
            })
            .collect()
    }

    /// Return the current block height.
    pub fn block_height(&self) -> u32 {
        self.0.borrow().block_height()
    }

    /// Return the current block timestamp.
    pub fn block_timestamp(&self) -> u64 {
        self.0.borrow().blocks_manager.get().timestamp
    }

    /// Returns an ethexe [`Program`] by `id`.
    pub fn get_program<ID: Into<ProgramIdWrapper>>(&self, id: ID) -> Option<Program<'_>> {
        let id = id.into().0;
        if ProgramsStorageManager::is_program(id) {
            Some(Program {
                id,
                manager: &self.0,
            })
        } else {
            None
        }
    }

    /// Returns last added program.
    pub fn last_program(&self) -> Option<Program<'_>> {
        self.programs().into_iter().next_back()
    }

    /// Returns a list of programs.
    pub fn programs(&self) -> Vec<Program<'_>> {
        ProgramsStorageManager::program_ids()
            .into_iter()
            .map(|id| Program {
                id,
                manager: &self.0,
            })
            .collect()
    }

    /// Detect if a program is active with given `id`.
    pub fn is_active_program<ID: Into<ProgramIdWrapper>>(&self, id: ID) -> bool {
        ProgramsStorageManager::is_active_program(id.into().0)
    }

    /// Saves code to the storage and returns its code hash.
    pub fn submit_local_code_file<P: AsRef<Path>>(&self, code_path: P) -> CodeId {
        let path = env::current_dir()
            .expect("Unable to get root directory of the project")
            .join(code_path)
            .clean();

        self.submit_code_file(path)
    }

    /// Saves code from file to the storage and returns its code hash.
    pub fn submit_code_file<P: AsRef<Path>>(&self, code_path: P) -> CodeId {
        let code = fs::read(&code_path).unwrap_or_else(|_| {
            usage_panic!(
                "Failed to read file {}",
                code_path.as_ref().to_string_lossy()
            )
        });

        self.submit_code(code)
    }

    /// Saves original code to the storage and returns its code hash.
    pub fn submit_code(&self, binary: impl Into<Vec<u8>>) -> CodeId {
        let code = binary.into();
        let code_id = CodeId::generate(code.as_ref());

        store_original_code(&mut self.0.borrow_mut(), code_id, code);

        code_id
    }

    /// Returns previously submitted original code by its code hash.
    pub fn submitted_code(&self, code_id: CodeId) -> Option<Vec<u8>> {
        self.0
            .borrow()
            .original_code(code_id)
            .map(|code| code.to_vec())
    }

    /// Mint balance to user with given `id` and `value`.
    pub fn mint_to<ID: Into<ProgramIdWrapper>>(&self, id: ID, value: Value) {
        let id = id.into().0;

        if ProgramsStorageManager::is_program(id) {
            usage_panic!(
                "Attempt to mint value to a program {id:?}. Please, use `System::transfer` instead"
            );
        }

        self.0.borrow_mut().mint_to(id, value);
    }

    /// Top up an ethexe program's executable balance.
    pub fn top_up_executable_balance(&self, program: impl Into<ProgramIdWrapper>, value: Value) {
        let program = program.into().0;

        self.0
            .borrow_mut()
            .ethexe_mut()
            .top_up_executable_balance(program, value);
    }

    /// Top up an ethexe program's reducible balance.
    pub fn top_up_balance(&self, program: impl Into<ProgramIdWrapper>, value: Value) {
        let program = program.into().0;

        self.0
            .borrow_mut()
            .ethexe_mut()
            .top_up_balance(program, value);
    }

    /// Inject a message into an initialized ethexe program.
    pub fn inject_message(
        &self,
        destination: impl Into<ProgramIdWrapper>,
        source: impl Into<ProgramIdWrapper>,
        payload: impl Into<Vec<u8>>,
        value: Value,
    ) -> MessageId {
        let destination = destination.into().0;
        let source = source.into().0;
        let mut manager = self.0.borrow_mut();

        manager.ethexe().ensure_can_queue_injected(destination);

        let block_number = manager.block_height() + 1;
        let message_id = MessageId::generate_from_user(
            block_number,
            source,
            manager.fetch_inc_message_nonce() as u128,
        );

        manager
            .ethexe_mut()
            .queue_injected(destination, message_id, source, payload.into(), value);

        message_id
    }

    /// Transfer balance from user with given `from` id to user with given `to` id.
    pub fn transfer(
        &self,
        from: impl Into<ProgramIdWrapper>,
        to: impl Into<ProgramIdWrapper>,
        value: Value,
        keep_alive: bool,
    ) {
        let from = from.into().0;
        let to = to.into().0;

        if ProgramsStorageManager::is_program(from) {
            usage_panic!(
                "Attempt to transfer from a program {from:?}. Please, provide `from` user id."
            );
        }

        Accounts::transfer(from, to, value, keep_alive);
    }

    /// Returns balance of user or ethexe program with given `id`.
    pub fn balance_of<ID: Into<ProgramIdWrapper>>(&self, id: ID) -> Value {
        let actor_id = id.into().0;
        let manager = self.0.borrow();

        if ProgramsStorageManager::is_program(actor_id) {
            return manager.ethexe().balance_of(actor_id);
        }

        manager.balance_of(actor_id)
    }
}

impl Drop for System {
    fn drop(&mut self) {
        SYSTEM_INITIALIZED.with_borrow_mut(|initialized| *initialized = false);
        let manager = self.0.borrow();
        manager.gas_tree.clear();
        manager.mailbox.clear();
        manager.task_pool.clear();
        manager.waitlist.clear();
        manager.blocks_manager.reset();
        manager.bank.clear();
        manager.nonce_manager.reset();
        manager.dispatches.clear();
        manager.dispatches_stash.clear();

        ProgramsStorageManager::clear();
        Accounts::clear();
        BridgeBuiltinStorage::clear();
    }
}

/// Builder for ethexe [`Program`].
#[must_use = "`build()` must be called at the end"]
#[derive(Debug, Clone)]
pub struct ProgramBuilder {
    code: Vec<u8>,
    meta: Option<Vec<u8>>,
    id: Option<ProgramIdWrapper>,
}

impl ProgramBuilder {
    /// Create an ethexe program from WASM binary.
    pub fn from_binary(code: impl Into<Vec<u8>>) -> Self {
        Self {
            code: code.into(),
            meta: None,
            id: None,
        }
    }

    /// Create an ethexe program instance from wasm file.
    pub fn from_file(path: impl AsRef<Path>) -> Self {
        Self::from_binary(fs::read(path).expect("Failed to read WASM file"))
    }

    fn wasm_path(optimized: bool) -> PathBuf {
        Self::wasm_path_from_binpath(optimized).unwrap_or_else(|| {
            crate::program::gbuild::wasm_path().expect("Unable to find built wasm")
        })
    }

    fn wasm_path_from_binpath(optimized: bool) -> Option<PathBuf> {
        let cwd = env::current_dir().expect("Unable to get current dir");
        let extension = if optimized { "opt.wasm" } else { "wasm" };
        let path_file = cwd.join(".binpath");
        let path_bytes = fs::read(path_file).ok()?;
        let mut relative_path: PathBuf =
            String::from_utf8(path_bytes).expect("Invalid path").into();
        relative_path.set_extension(extension);
        Some(cwd.join(relative_path))
    }

    fn inner_current(optimized: bool) -> Self {
        let path = env::current_dir()
            .expect("Unable to get root directory of the project")
            .join(Self::wasm_path(optimized))
            .clean();

        let filename = path.file_name().and_then(OsStr::to_str).unwrap_or_default();
        assert!(
            filename.ends_with(".wasm"),
            "File must have `.wasm` extension"
        );

        let code = fs::read(&path).unwrap_or_else(|_| panic!("Failed to read file {path:?}"));

        Self {
            code,
            meta: None,
            id: None,
        }
    }

    /// Get ethexe program of the root crate with provided `system`.
    pub fn current() -> Self {
        Self::inner_current(false)
    }

    /// Get optimized ethexe program of the root crate with provided `system`.
    pub fn current_opt() -> Self {
        Self::inner_current(true)
    }

    /// Set ID for future program.
    pub fn with_id(mut self, id: impl Into<ProgramIdWrapper>) -> Self {
        self.id = Some(id.into());
        self
    }

    /// Set metadata for future program.
    pub fn with_meta(mut self, meta: impl Into<Vec<u8>>) -> Self {
        self.meta = Some(meta.into());
        self
    }

    /// Set metadata for future program from file.
    pub fn with_meta_file(self, path: impl AsRef<Path>) -> Self {
        self.with_meta(fs::read(path).expect("Failed to read metadata file"))
    }

    /// Build ethexe program with set parameters.
    pub fn build(self, system: &System) -> Program<'_> {
        let Self { code, meta, id } = self;
        let id = id.unwrap_or_else(|| system.0.borrow_mut().free_id_nonce().into());

        let code_id = CodeId::generate(&code);
        store_original_code(&mut system.0.borrow_mut(), code_id, code);
        if let Some(metadata) = meta {
            system
                .0
                .borrow_mut()
                .meta_binaries
                .insert(code_id, metadata);
        }

        let expiration_block = system.block_height();
        Program::program_with_id(
            system,
            id,
            GTestProgram::Default {
                primary: PrimaryProgram::Active(ActiveProgram {
                    allocations_tree_len: 0,
                    code_id: code_id.cast(),
                    state: ProgramState::Uninitialized {
                        message_id: PLACEHOLDER_MESSAGE_ID,
                    },
                    expiration_block,
                    memory_infix: Default::default(),
                    gas_reservation_map: Default::default(),
                }),
            },
        )
    }

    pub(crate) fn build_ethexe_instrumented_code(
        original_code: Vec<u8>,
    ) -> (InstrumentedCode, CodeMetadata) {
        let schedule = Schedule::default();
        let code = Code::try_new(
            original_code,
            ethexe_runtime_common::VERSION,
            |module| schedule.rules(module),
            schedule.limits.stack_height,
            schedule.limits.data_segments_amount.into(),
            schedule.limits.type_section_len.into(),
            schedule.limits.parameters.into(),
            SyscallKind::Eth,
        )
        .expect("Failed to create ethexe Program from provided code");

        let (_, instrumented_code, code_metadata) = code.into_parts();
        (instrumented_code, code_metadata)
    }
}

/// Ethexe Gear program instance.
pub struct Program<'a> {
    pub(crate) manager: &'a RefCell<ExtManager>,
    pub(crate) id: ActorId,
}

impl<'a> Program<'a> {
    fn program_with_id<I: Into<ProgramIdWrapper> + Clone + Debug>(
        system: &'a System,
        id: I,
        program: GTestProgram,
    ) -> Self {
        let program_id = id.clone().into().0;

        if default_users_list().contains(&(program_id.into_bytes()[0] as u64)) {
            usage_panic!(
                "Can't create program with id {id:?}, because it's reserved for default users.\
                Please, use another id."
            )
        }

        let ethexe_code_id = match &program {
            GTestProgram::Default {
                primary: PrimaryProgram::Active(active_program),
            } => active_program.code_id,
            GTestProgram::Default { .. } => {
                usage_panic!("Only active programs can be created in ethexe gtest mode");
            }
            GTestProgram::Mock { .. } => {
                usage_panic!("Mock programs are not supported in ethexe gtest mode");
            }
        };

        if system.0.borrow_mut().store_program(program_id, program) {
            usage_panic!(
                "Can't create program with id {id:?}, because Program with this id already exists. \
                Please, use another id."
            )
        }

        let (instrumented_code, code_metadata) = {
            let manager = system.0.borrow();
            let code = manager
                .original_code(ethexe_code_id)
                .unwrap_or_else(|| panic!("missing original ethexe code {ethexe_code_id:?}"))
                .to_vec();

            ProgramBuilder::build_ethexe_instrumented_code(code)
        };

        system.0.borrow_mut().ethexe_mut().register_program(
            program_id,
            ethexe_code_id,
            instrumented_code,
            code_metadata,
        );

        Self {
            manager: &system.0,
            id: program_id,
        }
    }

    /// Get the ethexe program of the root crate with provided `system`.
    pub fn current(system: &'a System) -> Self {
        ProgramBuilder::current().build(system)
    }

    /// Get the ethexe program of the root crate with provided `system` and id.
    pub fn current_with_id<I: Into<ProgramIdWrapper> + Clone + Debug>(
        system: &'a System,
        id: I,
    ) -> Self {
        ProgramBuilder::current().with_id(id).build(system)
    }

    /// Get optimized ethexe program of the root crate with provided `system`.
    pub fn current_opt(system: &'a System) -> Self {
        ProgramBuilder::current_opt().build(system)
    }

    /// Create an ethexe program instance from wasm file.
    pub fn from_file<P: AsRef<Path>>(system: &'a System, path: P) -> Self {
        ProgramBuilder::from_file(path).build(system)
    }

    /// Create an ethexe program instance from wasm binary with given ID.
    pub fn from_binary_with_id<ID, B>(system: &'a System, id: ID, binary: B) -> Self
    where
        ID: Into<ProgramIdWrapper> + Clone + Debug,
        B: Into<Vec<u8>>,
    {
        ProgramBuilder::from_binary(binary)
            .with_id(id)
            .build(system)
    }

    /// Send message to the program.
    pub fn send<ID, C>(&self, from: ID, payload: C) -> MessageId
    where
        ID: Into<ProgramIdWrapper>,
        C: Codec,
    {
        self.send_with_value(from, payload, 0)
    }

    /// Send message to the program with value.
    pub fn send_with_value<ID, C>(&self, from: ID, payload: C, value: u128) -> MessageId
    where
        ID: Into<ProgramIdWrapper>,
        C: Codec,
    {
        self.send_bytes_with_value(from, payload.encode(), value)
    }

    /// Send message to the program with gas limit and value.
    pub fn send_with_gas<ID, P>(
        &self,
        from: ID,
        payload: P,
        gas_limit: u64,
        value: u128,
    ) -> MessageId
    where
        ID: Into<ProgramIdWrapper>,
        P: Encode,
    {
        self.send_bytes_with_gas_and_value(from, payload.encode(), gas_limit, value)
    }

    /// Send message to the program with bytes payload.
    pub fn send_bytes<ID, T>(&self, from: ID, payload: T) -> MessageId
    where
        ID: Into<ProgramIdWrapper>,
        T: Into<Vec<u8>>,
    {
        self.send_bytes_with_value(from, payload, 0)
    }

    /// Send the message to the program with bytes payload and value.
    pub fn send_bytes_with_value<ID, T>(&self, from: ID, payload: T, value: u128) -> MessageId
    where
        ID: Into<ProgramIdWrapper>,
        T: Into<Vec<u8>>,
    {
        self.send_bytes_with_gas_and_value(from, payload, MAX_USER_GAS_LIMIT, value)
    }

    /// Send the message to the program with bytes payload, gas limit and value.
    pub fn send_bytes_with_gas<ID, T>(
        &self,
        from: ID,
        payload: T,
        gas_limit: u64,
        value: u128,
    ) -> MessageId
    where
        ID: Into<ProgramIdWrapper>,
        T: Into<Vec<u8>>,
    {
        self.send_bytes_with_gas_and_value(from, payload, gas_limit, value)
    }

    fn send_bytes_with_gas_and_value<ID, T>(
        &self,
        from: ID,
        payload: T,
        gas_limit: u64,
        value: u128,
    ) -> MessageId
    where
        ID: Into<ProgramIdWrapper>,
        T: Into<Vec<u8>>,
    {
        if gas_limit != MAX_USER_GAS_LIMIT {
            usage_panic!("Explicit gas limits are not supported in ethexe gtest mode");
        }

        let mut system = self.manager.borrow_mut();
        let source = from.into().0;
        let block_number = system.block_height() + 1;
        let message_id = MessageId::generate_from_user(
            block_number,
            source,
            system.fetch_inc_message_nonce() as u128,
        );

        system
            .ethexe_mut()
            .queue_canonical(self.id, message_id, source, payload.into(), value);

        message_id
    }

    /// Get program id.
    pub fn id(&self) -> ActorId {
        self.id
    }

    /// Ethexe gtest currently does not support state reads.
    pub fn read_state_bytes(&self, _payload: Vec<u8>) -> Result<Vec<u8>> {
        usage_panic!("Program state reads are not supported in ethexe gtest mode");
    }

    /// Ethexe gtest currently does not support state reads.
    pub fn read_state<D: Decode, P: Encode>(&self, payload: P) -> Result<D> {
        let state_bytes = self.read_state_bytes(payload.encode())?;
        D::decode(&mut state_bytes.as_ref()).map_err(Into::into)
    }

    /// Returns the ethexe program reducible balance.
    pub fn balance(&self) -> Value {
        self.manager.borrow().ethexe().balance_of(self.id)
    }

    /// Returns the ethexe program executable balance.
    pub fn executable_balance(&self) -> Value {
        self.manager
            .borrow()
            .ethexe()
            .executable_balance_of(self.id)
    }
}
