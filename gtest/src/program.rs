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
    manager::{Balance, ExtManager, Program as InnerProgram, TestActor},
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

    /// Get program of the root crate with provided `system`.
    ///
    /// It looks up the wasm binary of the root crate that contains
    /// the current test, upload it to the testing system, then,
    /// returns the program instance.
    pub fn current(system: &'a System) -> Self {
        let nonce = system.0.borrow_mut().free_id_nonce();

        Self::current_with_id(system, nonce)
    }

    /// Get program of the root crate with provided `system` and
    /// initialize it with given `id`.
    ///
    /// See also [`Program::current`].
    pub fn current_with_id<I: Into<ProgramIdWrapper> + Clone + Debug>(
        system: &'a System,
        id: I,
    ) -> Self {
        Self::from_file_with_id(system, id, Self::wasm_path("wasm"))
    }

    /// Get optimized program of the root crate with provided `system`,
    ///
    /// See also [`Program::current`].
    pub fn current_opt(system: &'a System) -> Self {
        let nonce = system.0.borrow_mut().free_id_nonce();

        Self::current_opt_with_id(system, nonce)
    }

    /// Get optimized program of the root crate with provided `system` and
    /// initialize it with provided `id`.
    ///
    /// See also [`Program::current_with_id`].
    pub fn current_opt_with_id<I: Into<ProgramIdWrapper> + Clone + Debug>(
        system: &'a System,
        id: I,
    ) -> Self {
        Self::from_file_with_id(system, id, Self::wasm_path("opt.wasm"))
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
    pub fn mock_with_id<T: WasmProgram + 'static, I: Into<ProgramIdWrapper> + Clone + Debug>(
        system: &'a System,
        id: I,
        mock: T,
    ) -> Self {
        Self::program_with_id(system, id, InnerProgram::new_mock(mock))
    }

    /// Create a program instance from wasm file.
    ///
    /// See also [`Program::current`].
    pub fn from_file<P: AsRef<Path>>(system: &'a System, path: P) -> Self {
        let nonce = system.0.borrow_mut().free_id_nonce();

        Self::from_file_with_id(system, nonce, path)
    }

    /// Create a program from file and initialize it with provided
    /// `path` and `id`.
    ///
    /// `id` may be built from:
    /// - `u64`
    /// - `[u8; 32]`
    /// - `String`
    /// - `&str`
    /// - [`ProgramId`](https://docs.gear.rs/gear_core/ids/struct.ProgramId.html)
    ///   (from `gear_core` one's, not from `gstd`).
    ///
    /// # Examples
    ///
    /// From numeric id:
    ///
    /// ```no_run
    /// # use gtest::{Program, System};
    /// # let sys = System::new();
    /// let prog = Program::from_file_with_id(
    ///     &sys,
    ///     105,
    ///     "./target/wasm32-unknown-unknown/release/demo_ping.wasm",
    /// );
    /// ```
    ///
    /// From hex string starting with `0x`:
    ///
    /// ```no_run
    /// # use gtest::{Program, System};
    /// # let sys = System::new();
    /// let prog = Program::from_file_with_id(
    ///     &sys,
    ///     "0xe659a7a1628cdd93febc04a4e0646ea20e9f5f0ce097d9a05290d4a9e054df4e",
    ///     "./target/wasm32-unknown-unknown/release/demo_ping.wasm",
    /// );
    /// ```
    ///
    /// From hex string starting without `0x`:
    ///
    /// ```no_run
    /// # use gtest::{Program, System};
    /// # let sys = System::new();
    /// let prog = Program::from_file_with_id(
    ///     &sys,
    ///     "e659a7a1628cdd93febc04a4e0646ea20e9f5f0ce097d9a05290d4a9e054df5e",
    ///     "./target/wasm32-unknown-unknown/release/demo_ping.wasm",
    /// );
    /// ```
    ///
    /// From array of bytes (e.g. filled with `5`):
    ///
    /// ```no_run
    /// # use gtest::{Program, System};
    /// # let sys = System::new();
    /// let prog = Program::from_file_with_id(
    ///     &sys,
    ///     [5; 32],
    ///     "./target/wasm32-unknown-unknown/release/demo_ping.wasm",
    /// );
    /// ```
    ///
    /// # See also
    ///
    /// - [`Program::from_file`] for creating a program from file with default
    ///   id.
    #[track_caller]
    pub fn from_file_with_id<P: AsRef<Path>, I: Into<ProgramIdWrapper> + Clone + Debug>(
        system: &'a System,
        id: I,
        path: P,
    ) -> Self {
        let path = env::current_dir()
            .expect("Unable to get root directory of the project")
            .join(path)
            .clean();

        let filename = path.file_name().and_then(OsStr::to_str).unwrap_or_default();
        assert!(
            filename.ends_with(".wasm"),
            "File must have `.wasm` extension"
        );
        assert!(
            !filename.ends_with(".meta.wasm"),
            "Cannot load `.meta.wasm` file without `.opt.wasm` one. \
            Use Program::from_opt_and_meta() instead"
        );

        let code = fs::read(&path).unwrap_or_else(|_| panic!("Failed to read file {:?}", path));
        Self::from_opt_and_meta_code_with_id(system, id, code, None)
    }

    /// Create a program from optimized and metadata files.
    ///
    /// See also [`Program::from_file`].
    pub fn from_opt_and_meta<P: AsRef<Path>>(
        system: &'a System,
        optimized: P,
        metadata: P,
    ) -> Self {
        let nonce = system.0.borrow_mut().free_id_nonce();
        Self::from_opt_and_meta_with_id(system, nonce, optimized, metadata)
    }

    /// Create a program from optimized and metadata files and initialize
    /// it with given `id`.
    ///
    /// See also [`Program::from_file`].
    pub fn from_opt_and_meta_with_id<P: AsRef<Path>, I: Into<ProgramIdWrapper> + Clone + Debug>(
        system: &'a System,
        id: I,
        optimized: P,
        metadata: P,
    ) -> Self {
        let opt_code = read_file(optimized, ".opt.wasm");
        let meta_code = read_file(metadata, ".meta.wasm");

        Self::from_opt_and_meta_code_with_id(system, id, opt_code, Some(meta_code))
    }

    /// Create a program from optimized and metadata code and initialize
    /// it with given `id`.
    ///
    /// See also [`Program::from_file`].
    #[track_caller]
    pub fn from_opt_and_meta_code_with_id<I: Into<ProgramIdWrapper> + Clone + Debug>(
        system: &'a System,
        id: I,
        optimized: Vec<u8>,
        metadata: Option<Vec<u8>>,
    ) -> Self {
        let schedule = Schedule::default();
        let code = Code::try_new(
            optimized,
            schedule.instruction_weights.version,
            |module| schedule.rules(module),
            schedule.limits.stack_height,
        )
        .expect("Failed to create Program from code");

        let code_and_id: InstrumentedCodeAndId = CodeAndId::new(code).into();
        let (code, code_id) = code_and_id.into_parts();

        if let Some(metadata) = metadata {
            system
                .0
                .borrow_mut()
                .meta_binaries
                .insert(code_id, metadata);
        }

        let program_id = id.clone().into().0;
        let program = CoreProgram::new(program_id, Default::default(), code);

        Self::program_with_id(
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

    /// Send message to the program.
    pub fn send<ID: Into<ProgramIdWrapper>, C: Codec>(&self, from: ID, payload: C) -> RunResult {
        self.send_with_value(from, payload, 0)
    }

    /// Send message to the program with value.
    pub fn send_with_value<ID: Into<ProgramIdWrapper>, C: Codec>(
        &self,
        from: ID,
        payload: C,
        value: u128,
    ) -> RunResult {
        self.send_bytes_with_value(from, payload.encode(), value)
    }

    /// Send message to the program with bytes payload.
    pub fn send_bytes<ID: Into<ProgramIdWrapper>, T: AsRef<[u8]>>(
        &self,
        from: ID,
        payload: T,
    ) -> RunResult {
        self.send_bytes_with_value(from, payload, 0)
    }

    /// Send message to the program with bytes payload and value.
    #[track_caller]
    pub fn send_bytes_with_value<ID: Into<ProgramIdWrapper>, T: AsRef<[u8]>>(
        &self,
        from: ID,
        payload: T,
        value: u128,
    ) -> RunResult {
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
            payload.as_ref().to_vec().try_into().unwrap(),
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
        self.manager.borrow_mut().mint_to(&self.id(), value)
    }

    /// Returns the balance of the account.
    pub fn balance(&self) -> Balance {
        self.manager.borrow().balance_of(&self.id())
    }

    /// Returns the wasm path with extension.
    #[track_caller]
    fn wasm_path(extension: &str) -> PathBuf {
        let current_dir = env::current_dir().expect("Unable to get current dir");
        let path_file = current_dir.join(".binpath");
        let path_bytes = fs::read(path_file).expect("Unable to read path bytes");
        let mut relative_path: PathBuf =
            String::from_utf8(path_bytes).expect("Invalid path").into();
        relative_path.set_extension(extension);
        current_dir.join(relative_path)
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

#[track_caller]
fn read_file<P: AsRef<Path>>(path: P, extension: &str) -> Vec<u8> {
    let path = env::current_dir()
        .expect("Unable to get root directory of the project")
        .join(path)
        .clean();

    let filename = path.file_name().and_then(OsStr::to_str).unwrap_or_default();
    assert!(
        filename.ends_with(extension),
        "Wrong file extension: {extension}",
    );

    fs::read(&path).unwrap_or_else(|_| panic!("Failed to read file {:?}", path))
}

/// Calculate program id from code id and salt.
pub fn calculate_program_id(code_id: CodeId, salt: &[u8], id: Option<MessageId>) -> ProgramId {
    if let Some(id) = id {
        ProgramId::generate_from_program(code_id, salt, id)
    } else {
        ProgramId::generate_from_user(code_id, salt)
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

        let prog = Program::from_opt_and_meta_code_with_id(
            &sys,
            137,
            demo_futures_unordered::WASM_BINARY.to_vec(),
            None,
        );

        let init_msg_payload = String::from("InvalidInput");
        let run_result = prog.send(user_id, init_msg_payload);

        run_result.assert_panicked_with("Failed to load destination: Decode(Error)");

        let run_result = prog.send(user_id, String::from("should_be_skipped"));

        let expected_log = Log::error_builder(ErrorReplyReason::InactiveProgram)
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

        let mut prog = Program::from_opt_and_meta_code_with_id(
            &sys,
            137,
            demo_ping::WASM_BINARY.to_vec(),
            None,
        );

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

        let prog = Program::from_opt_and_meta_code_with_id(
            &sys,
            137,
            demo_piggy_bank::WASM_BINARY.to_vec(),
            None,
        );

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
        let prog = Program::from_opt_and_meta_code_with_id(
            &sys,
            2,
            demo_piggy_bank::WASM_BINARY.to_vec(),
            None,
        );

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

        let prog = Program::from_opt_and_meta_code_with_id(
            &sys,
            137,
            demo_piggy_bank::WASM_BINARY.to_vec(),
            None,
        );

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

        let mut prog =
            Program::from_opt_and_meta_code_with_id(&sys, 420, WASM_BINARY.to_vec(), None);

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

        let prog = Program::from_opt_and_meta_code_with_id(&sys, 420, WASM_BINARY.to_vec(), None);

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

        let prog = Program::from_opt_and_meta_code_with_id(&sys, 420, WASM_BINARY.to_vec(), None);

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
}
