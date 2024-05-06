// This file is part of Gear.

// Copyright (C) 2021-2024 Gear Technologies Inc.
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

use crate::{
    log::RunResult,
    manager::{Balance, ExtManager, MintMode, Program as InnerProgram, TestActor},
    system::System,
    Result,
};
use codec::{Codec, Decode, Encode};
use gear_core::{
    code::{Code, CodeAndId, InstrumentedCodeAndId},
    ids::{CodeId, MessageId, ProgramId},
    message::{Dispatch, DispatchKind, Message, SignalMessage},
    program::Program as CoreProgram,
};
use gear_core_errors::SignalCode;
use gear_utils::{MemoryPageDump, ProgramMemoryDump};
use gear_wasm_instrument::gas_metering::Schedule;
use path_clean::PathClean;
use std::{
    cell::RefCell,
    convert::TryInto,
    env,
    ffi::OsStr,
    fmt::Debug,
    fs,
    path::{Path, PathBuf},
};

/// Gas for gear programs.
#[derive(
    Default,
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    derive_more::Add,
    derive_more::AddAssign,
    derive_more::Sub,
    derive_more::SubAssign,
    derive_more::Mul,
    derive_more::MulAssign,
    derive_more::Div,
    derive_more::DivAssign,
    derive_more::Display,
)]
pub struct Gas(pub(crate) u64);

impl Gas {
    /// Gas with value zero.
    pub const fn zero() -> Self {
        Self(0)
    }

    /// Computes a + b, saturating at numeric bounds.
    pub const fn saturating_add(self, rhs: Self) -> Self {
        Self(self.0.saturating_add(rhs.0))
    }

    /// Computes a - b, saturating at numeric bounds.
    pub const fn saturating_sub(self, rhs: Self) -> Self {
        Self(self.0.saturating_sub(rhs.0))
    }

    /// Computes a * b, saturating at numeric bounds.
    pub const fn saturating_mul(self, rhs: Self) -> Self {
        Self(self.0.saturating_mul(rhs.0))
    }

    /// Computes a / b, saturating at numeric bounds.
    pub const fn saturating_div(self, rhs: Self) -> Self {
        Self(self.0.saturating_div(rhs.0))
    }
}

/// Trait for mocking gear programs.
///
/// See [`Program`] and [`Program::mock`] for the usages.
pub trait WasmProgram: Debug {
    /// Init wasm program with given `payload`.
    ///
    /// Returns `Ok(Some(payload))` if program has reply logic
    /// with given `payload`.
    ///
    /// If error occurs, the program will be terminated which
    /// means that `handle` and `handle_reply` will not be
    /// called.
    fn init(&mut self, payload: Vec<u8>) -> Result<Option<Vec<u8>>, &'static str>;
    /// Message handler with given `payload`.
    ///
    /// Returns `Ok(Some(payload))` if program has reply logic.
    fn handle(&mut self, payload: Vec<u8>) -> Result<Option<Vec<u8>>, &'static str>;
    /// Reply message handler with given `payload`.
    fn handle_reply(&mut self, payload: Vec<u8>) -> Result<(), &'static str>;
    /// Signal handler with given `payload`.
    fn handle_signal(&mut self, payload: Vec<u8>) -> Result<(), &'static str>;
    /// State of wasm program.
    ///
    /// See [`Program::read_state`] for the usage.
    fn state(&mut self) -> Result<Vec<u8>, &'static str>;
    /// Emit debug message in program with given `data`.
    ///
    /// Logging target `gwasm` is used in this method.
    fn debug(&mut self, data: &str) {
        log::debug!(target: "gwasm", "DEBUG: {data}");
    }
}

/// Wrapper for program id.
#[derive(Clone, Debug)]
pub struct ProgramIdWrapper(pub(crate) ProgramId);

impl<T: Into<ProgramIdWrapper> + Clone> PartialEq<T> for ProgramIdWrapper {
    fn eq(&self, other: &T) -> bool {
        self.0.eq(&other.clone().into().0)
    }
}

impl From<ProgramId> for ProgramIdWrapper {
    fn from(other: ProgramId) -> Self {
        Self(other)
    }
}

impl From<u64> for ProgramIdWrapper {
    fn from(other: u64) -> Self {
        let mut id = [0; 32];
        id[0..8].copy_from_slice(&other.to_le_bytes()[..]);
        Self(id.into())
    }
}

impl From<[u8; 32]> for ProgramIdWrapper {
    fn from(other: [u8; 32]) -> Self {
        Self(other.into())
    }
}

impl From<&[u8]> for ProgramIdWrapper {
    #[track_caller]
    fn from(other: &[u8]) -> Self {
        if other.len() != 32 {
            panic!("Invalid identifier: {:?}", other)
        }

        let mut bytes = [0; 32];
        bytes.copy_from_slice(other);

        bytes.into()
    }
}

impl From<Vec<u8>> for ProgramIdWrapper {
    fn from(other: Vec<u8>) -> Self {
        other[..].into()
    }
}

impl From<&Vec<u8>> for ProgramIdWrapper {
    fn from(other: &Vec<u8>) -> Self {
        other[..].into()
    }
}

impl From<String> for ProgramIdWrapper {
    fn from(other: String) -> Self {
        other[..].into()
    }
}

impl From<&str> for ProgramIdWrapper {
    #[track_caller]
    fn from(other: &str) -> Self {
        let id = other.strip_prefix("0x").unwrap_or(other);

        let mut bytes = [0u8; 32];

        if hex::decode_to_slice(id, &mut bytes).is_err() {
            panic!("Invalid identifier: {:?}", other)
        }

        Self(bytes.into())
    }
}

/// Construct state arguments.
///
/// Used for reading and decoding the program’s transformed state,
/// see [`Program::read_state_using_wasm`] for example.
#[macro_export]
macro_rules! state_args {
    () => {
        Option::<()>::None
    };
    ($single:expr) => {
        Some($single)
    };
    ($($multiple:expr),*) => {
        Some(($($multiple,)*))
    };
}

/// Construct encoded state arguments.
///
/// Used for reading the program’s transformed state as a byte vector,
/// see [`Program::read_state_bytes_using_wasm`] for example.
#[macro_export]
macro_rules! state_args_encoded {
    () => {
        Option::<Vec<u8>>::None
    };
    ($single:expr) => {
        {
            use $crate::codec::Encode;
            Some(($single).encode())
        }
    };
    ($($multiple:expr),*) => {
        {
            use $crate::codec::Encode;
            Some((($($multiple,)*)).encode())
        }
    };
}

/// Builder for [`Program`].
#[must_use = "`build()` must be called at the end"]
#[derive(Debug, Clone)]
pub struct ProgramBuilder {
    code: Vec<u8>,
    meta: Option<Vec<u8>>,
    id: Option<ProgramIdWrapper>,
}

impl ProgramBuilder {
    /// Create program from WASM binary.
    pub fn from_binary(code: impl Into<Vec<u8>>) -> Self {
        Self {
            code: code.into(),
            meta: None,
            id: None,
        }
    }

    /// Create a program instance from wasm file.
    #[track_caller]
    pub fn from_file(path: impl AsRef<Path>) -> Self {
        Self::from_binary(fs::read(path).expect("Failed to read WASM file"))
    }

    fn wasm_path(optimized: bool) -> PathBuf {
        Self::wasm_path_from_binpath(optimized)
            .unwrap_or_else(|| gbuild::wasm_path().expect("Unable to find built wasm"))
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
            // TODO: consider to use `.canonicalize()` instead
            .clean();

        let filename = path.file_name().and_then(OsStr::to_str).unwrap_or_default();
        assert!(
            filename.ends_with(".wasm"),
            "File must have `.wasm` extension"
        );

        let code = fs::read(&path).unwrap_or_else(|_| panic!("Failed to read file {:?}", path));

        Self {
            code,
            meta: None,
            id: None,
        }
    }

    /// Get program of the root crate with provided `system`.
    ///
    /// It looks up the wasm binary of the root crate that contains
    /// the current test, uploads it to the testing system, then
    /// returns the program instance.
    #[track_caller]
    pub fn current() -> Self {
        Self::inner_current(false)
    }

    /// Get optimized program of the root crate with provided `system`,
    ///
    /// See also [`ProgramBuilder::current`].
    #[track_caller]
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
    ///
    /// See also [`ProgramBuilder::with_meta`].
    #[track_caller]
    pub fn with_meta_file(self, path: impl AsRef<Path>) -> Self {
        self.with_meta(fs::read(path).expect("Failed to read metadata file"))
    }

    /// Build program with set parameters.
    #[track_caller]
    pub fn build(self, system: &System) -> Program {
        let id = self
            .id
            .unwrap_or_else(|| system.0.borrow_mut().free_id_nonce().into());

        let schedule = Schedule::default();
        let code = Code::try_new(
            self.code,
            schedule.instruction_weights.version,
            |module| schedule.rules(module),
            schedule.limits.stack_height,
        )
        .expect("Failed to create Program from code");

        let code_and_id: InstrumentedCodeAndId = CodeAndId::new(code).into();
        let (code, code_id) = code_and_id.into_parts();

        if let Some(metadata) = self.meta {
            system
                .0
                .borrow_mut()
                .meta_binaries
                .insert(code_id, metadata);
        }

        let program = CoreProgram::new(id.0, Default::default(), code);

        Program::program_with_id(
            system,
            id,
            InnerProgram::Genuine {
                program,
                code_id,
                pages_data: Default::default(),
                gas_reservation_map: Default::default(),
            },
        )
    }
}

/// Gear program instance.
///
/// ```ignore
/// use gtest::{System, Program};
///
/// // Create a testing system.
/// let system = System::new();
///
/// // Get the current program of the testing system.
/// let program = Program::current(&system);
///
/// // Initialize the program from user 42 with message "init program".
/// let _result = program.send(42, "init program");
/// ```
pub struct Program<'a> {
    pub(crate) manager: &'a RefCell<ExtManager>,
    pub(crate) id: ProgramId,
}

impl<'a> Program<'a> {
    fn program_with_id<I: Into<ProgramIdWrapper> + Clone + Debug>(
        system: &'a System,
        id: I,
        program: InnerProgram,
    ) -> Self {
        let program_id = id.clone().into().0;

        if system
            .0
            .borrow_mut()
            .store_new_actor(program_id, program, None)
            .is_some()
        {
            panic!(
                "Can't create program with id {:?}, because Program with this id already exists",
                id
            )
        }

        Self {
            manager: &system.0,
            id: program_id,
        }
    }

    /// Get the program of the root crate with provided `system`.
    ///
    /// See [`ProgramBuilder::current`]
    pub fn current(system: &'a System) -> Self {
        ProgramBuilder::current().build(system)
    }

    /// Get the program of the root crate with provided `system` and
    /// initialize it with given `id`.
    ///
    /// See also [`Program::current`].
    pub fn current_with_id<I: Into<ProgramIdWrapper> + Clone + Debug>(
        system: &'a System,
        id: I,
    ) -> Self {
        ProgramBuilder::current().with_id(id).build(system)
    }

    /// Get optimized program of the root crate with provided `system`,
    ///
    /// See also [`Program::current`].
    pub fn current_opt(system: &'a System) -> Self {
        ProgramBuilder::current_opt().build(system)
    }

    /// Create a program instance from wasm file.
    ///
    /// See also [`Program::current`].
    pub fn from_file<P: AsRef<Path>>(system: &'a System, path: P) -> Self {
        ProgramBuilder::from_file(path).build(system)
    }

    /// Create a program instance from wasm file with given ID.
    ///
    /// See also [`Program::from_file`].
    pub fn from_binary_with_id<ID, B>(system: &'a System, id: ID, binary: B) -> Self
    where
        ID: Into<ProgramIdWrapper> + Clone + Debug,
        B: Into<Vec<u8>>,
    {
        ProgramBuilder::from_binary(binary)
            .with_id(id)
            .build(system)
    }

    /// Mock a program with provided `system` and `mock`.
    ///
    /// See [`WasmProgram`] for more details.
    pub fn mock<T: WasmProgram + 'static>(system: &'a System, mock: T) -> Self {
        let nonce = system.0.borrow_mut().free_id_nonce();

        Self::mock_with_id(system, nonce, mock)
    }

    /// Create a mock program with provided `system` and `mock`,
    /// and initialize it with provided `id`.
    ///
    /// See also [`Program::mock`].
    pub fn mock_with_id<ID, T>(system: &'a System, id: ID, mock: T) -> Self
    where
        T: WasmProgram + 'static,
        ID: Into<ProgramIdWrapper> + Clone + Debug,
    {
        Self::program_with_id(system, id, InnerProgram::new_mock(mock))
    }

    /// Send message to the program.
    pub fn send<ID, C>(&self, from: ID, payload: C) -> RunResult
    where
        ID: Into<ProgramIdWrapper>,
        C: Codec,
    {
        self.send_with_value(from, payload, 0)
    }

    /// Send message to the program with value.
    pub fn send_with_value<ID, C>(&self, from: ID, payload: C, value: u128) -> RunResult
    where
        ID: Into<ProgramIdWrapper>,
        C: Codec,
    {
        self.send_bytes_with_value(from, payload.encode(), value)
    }

    /// Send message to the program with bytes payload.
    pub fn send_bytes<ID, T>(&self, from: ID, payload: T) -> RunResult
    where
        ID: Into<ProgramIdWrapper>,
        T: Into<Vec<u8>>,
    {
        self.send_bytes_with_value(from, payload, 0)
    }

    /// Send the message to the program with bytes payload and value.
    #[track_caller]
    pub fn send_bytes_with_value<ID, T>(&self, from: ID, payload: T, value: u128) -> RunResult
    where
        ID: Into<ProgramIdWrapper>,
        T: Into<Vec<u8>>,
    {
        let mut system = self.manager.borrow_mut();

        let source = from.into().0;

        let message = Message::new(
            MessageId::generate_from_user(
                system.block_info.height,
                source,
                system.fetch_inc_message_nonce() as u128,
            ),
            source,
            self.id,
            payload.into().try_into().unwrap(),
            Some(u64::MAX),
            value,
            None,
        );

        let mut actors = system.actors.borrow_mut();
        let (actor, _) = actors.get_mut(&self.id).expect("Can't fail");

        let kind = if let TestActor::Uninitialized(id @ None, _) = actor {
            *id = Some(message.id());
            DispatchKind::Init
        } else {
            DispatchKind::Handle
        };

        drop(actors);
        system.validate_and_run_dispatch(Dispatch::new(kind, message))
    }

    /// Send signal to the program.
    #[track_caller]
    pub fn send_signal<ID: Into<ProgramIdWrapper>>(&self, from: ID, code: SignalCode) -> RunResult {
        let mut system = self.manager.borrow_mut();

        let source = from.into().0;

        let origin_msg_id = MessageId::generate_from_user(
            system.block_info.height,
            source,
            system.fetch_inc_message_nonce() as u128,
        );
        let message = SignalMessage::new(origin_msg_id, code);

        let mut actors = system.actors.borrow_mut();
        let (actor, _) = actors.get_mut(&self.id).expect("Can't fail");

        if let TestActor::Uninitialized(id @ None, _) = actor {
            *id = Some(message.id());
        };

        drop(actors);
        let dispatch = message.into_dispatch(origin_msg_id, self.id);
        system.validate_and_run_dispatch(dispatch)
    }

    /// Get program id.
    pub fn id(&self) -> ProgramId {
        self.id
    }

    /// Reads the program’s state as a byte vector.
    pub fn read_state_bytes(&self, payload: Vec<u8>) -> Result<Vec<u8>> {
        self.manager
            .borrow_mut()
            .read_state_bytes(payload, &self.id)
    }

    /// Reads the program’s transformed state as a byte vector. The transformed
    /// state is a result of applying the `fn_name` function from the `wasm`
    /// binary with the optional `argument`.
    ///
    /// # Usage
    /// You can pass arguments as `Option<(arg1, arg2, ...).encode()>` or by
    /// using [`state_args_encoded`] macro.
    ///
    /// # Examples
    ///
    /// ```
    /// # use gtest::{state_args_encoded, Program, System, WasmProgram, Result};
    /// # use codec::Encode;
    /// # fn doctest() -> Result<()> {
    /// # #[derive(Debug)]
    /// # struct MockWasm {}
    /// #
    /// # impl WasmProgram for MockWasm {
    /// #     fn init(&mut self, _payload: Vec<u8>) -> Result<Option<Vec<u8>>, &'static str> { unimplemented!() }
    /// #     fn handle(&mut self, _payload: Vec<u8>) -> Result<Option<Vec<u8>>, &'static str> { unimplemented!() }
    /// #     fn handle_reply(&mut self, _payload: Vec<u8>) -> Result<(), &'static str> {unimplemented!() }
    /// #     fn handle_signal(&mut self, _payload: Vec<u8>) -> Result<(), &'static str> { unimplemented!()  }
    /// #     fn state(&mut self) -> Result<Vec<u8>, &'static str> { unimplemented!()  }
    /// #  }
    /// # let system = System::new();
    /// # let program = Program::mock(&system, MockWasm { });
    /// # let ARG_1 = 0u8;
    /// # let ARG_2 = 0u8;
    /// //Read state bytes with no arguments passed to wasm.
    /// # let WASM = vec![];
    /// let _ = program.read_state_bytes_using_wasm(Default::default(), "fn_name", WASM, Option::<Vec<u8>>::None)?;
    /// # let WASM = vec![];
    /// let _ = program.read_state_bytes_using_wasm(Default::default(), "fn_name", WASM, state_args_encoded!())?;
    /// // Read state bytes with one argument passed to wasm.
    /// # let WASM = vec![];
    /// let _ = program.read_state_bytes_using_wasm(Default::default(), "fn_name", WASM, Some(ARG_1.encode()))?;
    /// # let WASM = vec![];
    /// let _ = program.read_state_bytes_using_wasm(Default::default(), "fn_name", WASM, state_args_encoded!(ARG_1))?;
    /// // Read state bytes with multiple arguments passed to wasm.
    /// # let WASM = vec![];
    /// let _ = program.read_state_bytes_using_wasm(Default::default(), "fn_name", WASM, Some((ARG_1, ARG_2).encode()))?;
    /// # let WASM = vec![];
    /// let _ = program.read_state_bytes_using_wasm(Default::default(), "fn_name", WASM, state_args_encoded!(ARG_1, ARG_2))?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn read_state_bytes_using_wasm(
        &self,
        payload: Vec<u8>,
        fn_name: &str,
        wasm: Vec<u8>,
        args: Option<Vec<u8>>,
    ) -> Result<Vec<u8>> {
        self.manager
            .borrow_mut()
            .read_state_bytes_using_wasm(payload, &self.id, fn_name, wasm, args)
    }

    /// Reads and decodes the program's state .
    pub fn read_state<D: Decode, P: Encode>(&self, payload: P) -> Result<D> {
        let state_bytes = self.read_state_bytes(payload.encode())?;
        D::decode(&mut state_bytes.as_ref()).map_err(Into::into)
    }

    /// Reads and decodes the program’s transformed state. The transformed state
    /// is a result of applying the `fn_name` function from the `wasm`
    /// binary with the optional `argument`.
    ///
    /// # Usage
    /// You can pass arguments as `Option<(arg1, arg2, ...)>` or by
    /// using [`state_args`] macro.
    ///
    /// # Examples
    ///
    /// ```
    /// # use gtest::{state_args, Program, System, WasmProgram, Result};
    /// # fn doctest() -> Result<()> {
    /// # #[derive(Debug)]
    /// # struct MockWasm;
    /// #
    /// # impl WasmProgram for MockWasm {
    /// #     fn init(&mut self, _payload: Vec<u8>) -> Result<Option<Vec<u8>>, &'static str> { unimplemented!() }
    /// #     fn handle(&mut self, _payload: Vec<u8>) -> Result<Option<Vec<u8>>, &'static str> { unimplemented!() }
    /// #     fn handle_reply(&mut self, _payload: Vec<u8>) -> Result<(), &'static str> {unimplemented!() }
    /// #     fn handle_signal(&mut self, _payload: Vec<u8>) -> Result<(), &'static str> { unimplemented!()  }
    /// #     fn state(&mut self) -> Result<Vec<u8>, &'static str> { unimplemented!()  }
    /// #  }
    /// # let system = System::new();
    /// # let program = Program::mock(&system, MockWasm);
    /// # let ARG_1 = 0u8;
    /// # let ARG_2 = 0u8;
    /// //Read state bytes with no arguments passed to wasm.
    /// # let WASM = vec![];
    /// let _ = program.read_state_using_wasm(Vec::<u8>::default(), "fn_name", WASM, Option::<()>::None)?;
    /// # let WASM = vec![];
    /// let _ = program.read_state_using_wasm(Vec::<u8>::default(), "fn_name", WASM, state_args!())?;
    /// // Read state bytes with one argument passed to wasm.
    /// # let WASM = vec![];
    /// let _ = program.read_state_using_wasm(Vec::<u8>::default(), "fn_name", WASM, Some(ARG_1))?;
    /// # let WASM = vec![];
    /// let _ = program.read_state_using_wasm(Vec::<u8>::default(), "fn_name", WASM, state_args!(ARG_1))?;
    /// // Read state bytes with multiple arguments passed to wasm.
    /// # let WASM = vec![];
    /// let _ = program.read_state_using_wasm(Vec::<u8>::default(), "fn_name", WASM, Some((ARG_1, ARG_2)))?;
    /// # let WASM = vec![];
    /// let _ = program.read_state_using_wasm(Vec::<u8>::default(), "fn_name", WASM, state_args!(ARG_1, ARG_2))?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn read_state_using_wasm<E: Encode, P: Encode, D: Decode>(
        &self,
        payload: P,
        fn_name: &str,
        wasm: Vec<u8>,
        argument: Option<E>,
    ) -> Result<D> {
        let argument_bytes = argument.map(|arg| arg.encode());
        let state_bytes =
            self.read_state_bytes_using_wasm(payload.encode(), fn_name, wasm, argument_bytes)?;
        D::decode(&mut state_bytes.as_ref()).map_err(Into::into)
    }

    /// Mint balance to the account.
    pub fn mint(&mut self, value: Balance) {
        self.manager
            .borrow_mut()
            .mint_to(&self.id(), value, MintMode::KeepAlive)
    }

    /// Returns the balance of the account.
    pub fn balance(&self) -> Balance {
        self.manager.borrow().balance_of(&self.id())
    }

    /// Save the program's memory to path.
    pub fn save_memory_dump(&self, path: impl AsRef<Path>) {
        let manager = self.manager.borrow();
        let mem = manager.read_memory_pages(&self.id);
        let balance = manager.balance_of(&self.id);

        ProgramMemoryDump {
            balance,
            reserved_balance: 0,
            pages: mem
                .iter()
                .map(|(page_number, page_data)| {
                    MemoryPageDump::new(*page_number, page_data.clone())
                })
                .collect(),
        }
        .save_to_file(path);
    }

    /// Load the program's memory from path.
    pub fn load_memory_dump(&mut self, path: impl AsRef<Path>) {
        let memory_dump = ProgramMemoryDump::load_from_file(path);
        let mem = memory_dump
            .pages
            .into_iter()
            .map(MemoryPageDump::into_gear_page)
            .collect();

        // @TODO : add support for gas reservation when implemented
        let balance = memory_dump
            .balance
            .saturating_add(memory_dump.reserved_balance);

        self.manager
            .borrow_mut()
            .update_storage_pages(&self.id, mem);
        self.manager
            .borrow_mut()
            .override_balance(&self.id, balance);
    }
}

/// Calculate program id from code id and salt.
pub fn calculate_program_id(code_id: CodeId, salt: &[u8], id: Option<MessageId>) -> ProgramId {
    if let Some(id) = id {
        ProgramId::generate_from_program(code_id, salt, id)
    } else {
        ProgramId::generate_from_user(code_id, salt)
    }
}

/// `cargo-gbuild` utils
pub mod gbuild {
    use crate::{error::TestError as Error, Result};
    use cargo_toml::Manifest;
    use std::{path::PathBuf, process::Command};

    /// Search program wasm from
    ///
    /// - `target/gbuild`
    /// - `$WORKSPACE_ROOT/target/gbuild`
    ///
    /// NOTE: Release or Debug is decided by the users
    /// who run the command `cargo-gbuild`.
    pub fn wasm_path() -> Result<PathBuf> {
        let target = etc::find_up("target")
            .map_err(|_| Error::GbuildArtifactNotFound("Could not find target folder".into()))?;
        let manifest = Manifest::from_path(
            etc::find_up("Cargo.toml")
                .map_err(|_| Error::GbuildArtifactNotFound("Could not find manifest".into()))?,
        )
        .map_err(|_| Error::GbuildArtifactNotFound("Failed to parse manifest".into()))?;

        let artifact = target
            .join(format!(
                "gbuild/{}",
                manifest.package().name().replace('-', "_")
            ))
            .with_extension("wasm");

        if artifact.exists() {
            Ok(artifact)
        } else {
            Err(Error::GbuildArtifactNotFound(format!(
                "Program artifact not exist, {artifact:?}"
            )))
        }
    }

    /// Ensure the current project has been built by `cargo-gbuild`.
    pub fn ensure_gbuild() {
        if wasm_path().is_err() {
            let manifest = etc::find_up("Cargo.toml").expect("Unable to find project manifest.");
            if !Command::new("cargo")
                // NOTE: The `cargo-gbuild` command could be overridden by user defined alias,
                // this is a workaround for our workspace, for the details, see: issue #10049
                // <https://github.com/rust-lang/cargo/issues/10049>.
                .current_dir(
                    manifest
                        .ancestors()
                        .nth(2)
                        .expect("The project is under the root directory"),
                )
                .args(["gbuild", "-m"])
                .arg(&manifest)
                .status()
                .expect("cargo-gbuild is not installed, try `cargo install cargo-gbuild` first.")
                .success()
            {
                panic!("Error occurs while compiling the current program, please run `cargo gbuild` directly for the current project to detect the problem, manifest path: {manifest:?}")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Program;
    use crate::{Log, System};
    use gear_core_errors::ErrorReplyReason;

    #[test]
    fn test_handle_messages_to_failing_program() {
        let sys = System::new();
        sys.init_logger();

        let user_id = 100;

        let prog = Program::from_binary_with_id(&sys, 137, demo_futures_unordered::WASM_BINARY);

        let init_msg_payload = String::from("InvalidInput");
        let run_result = prog.send(user_id, init_msg_payload);

        run_result.assert_panicked_with("Failed to load destination: Decode(Error)");

        let run_result = prog.send(user_id, String::from("should_be_skipped"));

        let expected_log = Log::error_builder(ErrorReplyReason::InactiveActor)
            .source(prog.id())
            .dest(user_id);

        assert!(!run_result.main_failed());
        assert!(run_result.contains(&expected_log));
    }

    #[test]
    fn simple_balance() {
        let sys = System::new();
        sys.init_logger();

        let user_id = 42;
        sys.mint_to(user_id, 10 * crate::EXISTENTIAL_DEPOSIT);
        assert_eq!(sys.balance_of(user_id), 10 * crate::EXISTENTIAL_DEPOSIT);

        let mut prog = Program::from_binary_with_id(&sys, 137, demo_ping::WASM_BINARY);

        prog.mint(2 * crate::EXISTENTIAL_DEPOSIT);
        assert_eq!(prog.balance(), 2 * crate::EXISTENTIAL_DEPOSIT);

        prog.send_with_value(user_id, "init".to_string(), crate::EXISTENTIAL_DEPOSIT);
        assert_eq!(prog.balance(), 3 * crate::EXISTENTIAL_DEPOSIT);
        assert_eq!(sys.balance_of(user_id), 9 * crate::EXISTENTIAL_DEPOSIT);

        prog.send_with_value(user_id, "PING".to_string(), 2 * crate::EXISTENTIAL_DEPOSIT);
        assert_eq!(prog.balance(), 5 * crate::EXISTENTIAL_DEPOSIT);
        assert_eq!(sys.balance_of(user_id), 7 * crate::EXISTENTIAL_DEPOSIT);
    }

    #[test]
    fn piggy_bank() {
        let sys = System::new();
        sys.init_logger();

        let receiver = 42;
        let sender0 = 43;
        let sender1 = 44;
        let sender2 = 45;

        // Top-up senders balances
        sys.mint_to(sender0, 20 * crate::EXISTENTIAL_DEPOSIT);
        sys.mint_to(sender1, 20 * crate::EXISTENTIAL_DEPOSIT);
        sys.mint_to(sender2, 20 * crate::EXISTENTIAL_DEPOSIT);

        let prog = Program::from_binary_with_id(&sys, 137, demo_piggy_bank::WASM_BINARY);

        prog.send_bytes(receiver, b"init");
        assert_eq!(prog.balance(), 0);

        // Send values to the program
        prog.send_bytes_with_value(sender0, b"insert", 2 * crate::EXISTENTIAL_DEPOSIT);
        assert_eq!(sys.balance_of(sender0), 18 * crate::EXISTENTIAL_DEPOSIT);
        prog.send_bytes_with_value(sender1, b"insert", 4 * crate::EXISTENTIAL_DEPOSIT);
        assert_eq!(sys.balance_of(sender1), 16 * crate::EXISTENTIAL_DEPOSIT);
        prog.send_bytes_with_value(sender2, b"insert", 6 * crate::EXISTENTIAL_DEPOSIT);
        assert_eq!(sys.balance_of(sender2), 14 * crate::EXISTENTIAL_DEPOSIT);

        // Check program's balance
        assert_eq!(prog.balance(), (2 + 4 + 6) * crate::EXISTENTIAL_DEPOSIT);

        // Request to smash the piggy bank and send the value to the receiver address
        prog.send_bytes(receiver, b"smash");
        sys.claim_value_from_mailbox(receiver);
        assert_eq!(
            sys.balance_of(receiver),
            (2 + 4 + 6) * crate::EXISTENTIAL_DEPOSIT
        );

        // Check program's balance is empty
        assert_eq!(prog.balance(), 0);
    }

    #[test]
    #[should_panic(
        expected = "An attempt to mint value (1) less than existential deposit (10000000000000)"
    )]
    fn mint_less_than_deposit() {
        System::new().mint_to(1, 1);
    }

    #[test]
    #[should_panic(expected = "Insufficient value: user \
    (0x0100000000000000000000000000000000000000000000000000000000000000) tries \
    to send (10000000000001) value, while his balance (10000000000000)")]
    fn fails_on_insufficient_balance() {
        let sys = System::new();

        let user = 1;
        let prog = Program::from_binary_with_id(&sys, 2, demo_piggy_bank::WASM_BINARY);

        assert_eq!(sys.balance_of(user), 0);
        sys.mint_to(user, crate::EXISTENTIAL_DEPOSIT);
        assert_eq!(sys.balance_of(user), crate::EXISTENTIAL_DEPOSIT);

        prog.send_bytes_with_value(user, b"init", crate::EXISTENTIAL_DEPOSIT + 1);
    }

    #[test]
    fn claim_zero_value() {
        let sys = System::new();
        sys.init_logger();

        let sender = 42;
        let receiver = 84;

        sys.mint_to(sender, 20 * crate::EXISTENTIAL_DEPOSIT);

        let prog = Program::from_binary_with_id(&sys, 137, demo_piggy_bank::WASM_BINARY);

        prog.send_bytes(receiver, b"init");

        // Get zero value to the receiver's mailbox
        prog.send_bytes(receiver, b"smash");

        // Get the value > ED to the receiver's mailbox
        prog.send_bytes_with_value(sender, b"insert", 2 * crate::EXISTENTIAL_DEPOSIT);
        prog.send_bytes(receiver, b"smash");

        // Check receiver's balance
        sys.claim_value_from_mailbox(receiver);
        assert_eq!(sys.balance_of(receiver), 2 * crate::EXISTENTIAL_DEPOSIT);
    }

    struct CleanupFolderOnDrop {
        path: String,
    }

    impl Drop for CleanupFolderOnDrop {
        #[track_caller]
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.path).expect("Failed to cleanup after test")
        }
    }

    #[test]
    fn save_load_memory_dump() {
        use demo_custom::{InitMessage, WASM_BINARY};
        let sys = System::new();
        sys.init_logger();

        let mut prog = Program::from_binary_with_id(&sys, 420, WASM_BINARY);

        let signer = 42;

        // Init capacitor with limit = 15
        prog.send(signer, InitMessage::Capacitor("15".to_string()));

        // Charge capacitor with charge = 10
        let response = dbg!(prog.send_bytes(signer, b"10"));
        let log = Log::builder()
            .source(prog.id())
            .dest(signer)
            .payload_bytes([]);
        assert!(response.contains(&log));

        let cleanup = CleanupFolderOnDrop {
            path: "./296c6962726".to_string(),
        };
        prog.save_memory_dump("./296c6962726/demo_custom.dump");

        // Charge capacitor with charge = 10
        let response = prog.send_bytes(signer, b"10");
        let log = Log::builder()
            .source(prog.id())
            .dest(signer)
            .payload_bytes("Discharged: 20");
        // dbg!(log.clone());
        assert!(response.contains(&log));
        sys.claim_value_from_mailbox(signer);

        prog.load_memory_dump("./296c6962726/demo_custom.dump");
        drop(cleanup);

        // Charge capacitor with charge = 10
        let response = prog.send_bytes(signer, b"10");
        let log = Log::builder()
            .source(prog.id())
            .dest(signer)
            .payload_bytes("Discharged: 20");
        assert!(response.contains(&log));
        sys.claim_value_from_mailbox(signer);
    }

    #[test]
    fn process_wait_for() {
        use demo_custom::{InitMessage, WASM_BINARY};
        let sys = System::new();
        sys.init_logger();

        let prog = Program::from_binary_with_id(&sys, 420, WASM_BINARY);

        let signer = 42;

        // Init simple waiter
        prog.send(signer, InitMessage::SimpleWaiter);

        // Invoke `exec::wait_for` when running for the first time
        let result = prog.send_bytes(signer, b"doesn't matter");

        // No log entries as the program is waiting
        assert!(result.log().is_empty());

        // Spend 20 blocks and make the waiter to wake up
        let results = sys.spend_blocks(20);

        let log = Log::builder()
            .source(prog.id())
            .dest(signer)
            .payload_bytes("hello");

        assert!(results.iter().any(|result| result.contains(&log)));
    }

    #[test]
    #[should_panic]
    fn reservations_limit() {
        use demo_custom::{InitMessage, WASM_BINARY};
        let sys = System::new();
        sys.init_logger();

        let prog = Program::from_binary_with_id(&sys, 420, WASM_BINARY);

        let signer = 42;

        // Init reserver
        prog.send(signer, InitMessage::Reserver);

        for _ in 0..258 {
            // Reserve
            let result = prog.send_bytes(signer, b"reserve");
            assert!(!result.main_failed());

            // Spend
            let result = prog.send_bytes(signer, b"send from reservation");
            assert!(!result.main_failed());
        }
    }

    #[test]
    fn test_handle_exit_with_zero_balance() {
        use demo_constructor::{demo_exit_handle, WASM_BINARY};

        let sys = System::new();
        sys.init_logger();

        let user_id = [42; 32];
        let prog = Program::from_binary_with_id(&sys, 137, WASM_BINARY);

        let run_result = prog.send(user_id, demo_exit_handle::scheme());
        assert!(!run_result.main_failed());

        let run_result = prog.send_bytes(user_id, []);
        assert!(!run_result.main_failed());
    }
}
