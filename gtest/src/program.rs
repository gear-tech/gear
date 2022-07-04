// This file is part of Gear.

// Copyright (C) 2021-2022 Gear Technologies Inc.
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
    manager::{Actor, Balance, ExtManager, Program as InnerProgram},
    system::System,
    Result,
};
use codec::{Codec, Decode, Encode};
use gear_core::{
    code::{Code, CodeAndId, InstrumentedCodeAndId},
    ids::{CodeId, MessageId, ProgramId},
    message::{Dispatch, DispatchKind, Message},
    program::Program as CoreProgram,
};
use gear_wasm_builder::optimize::Optimizer;
use path_clean::PathClean;
use std::{
    cell::RefCell,
    env,
    ffi::OsStr,
    fmt::Debug,
    fs,
    path::{Path, PathBuf},
};
use wasm_instrument::gas_metering::ConstantCostRules;

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
    pub const fn zero() -> Self {
        Self(0)
    }

    pub const fn saturating_add(self, rhs: Self) -> Self {
        Self(self.0.saturating_add(rhs.0))
    }

    pub const fn saturating_sub(self, rhs: Self) -> Self {
        Self(self.0.saturating_sub(rhs.0))
    }

    pub const fn saturating_mul(self, rhs: Self) -> Self {
        Self(self.0.saturating_mul(rhs.0))
    }

    pub const fn saturating_div(self, rhs: Self) -> Self {
        Self(self.0.saturating_div(rhs.0))
    }
}

pub trait WasmProgram: Debug {
    fn init(&mut self, payload: Vec<u8>) -> Result<Option<Vec<u8>>, &'static str>;
    fn handle(&mut self, payload: Vec<u8>) -> Result<Option<Vec<u8>>, &'static str>;
    fn handle_reply(&mut self, payload: Vec<u8>) -> Result<Option<Vec<u8>>, &'static str>;
    fn meta_state(&mut self, payload: Option<Vec<u8>>) -> Result<Vec<u8>, &'static str>;
    fn debug(&mut self, data: &str) {
        logger::debug!(target: "gwasm", "DEBUG: {}", data);
    }
}

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

impl From<String> for ProgramIdWrapper {
    fn from(other: String) -> Self {
        other[..].into()
    }
}

impl From<&str> for ProgramIdWrapper {
    fn from(other: &str) -> Self {
        let id = other.strip_prefix("0x").unwrap_or(other);

        let mut bytes = [0u8; 32];

        if hex::decode_to_slice(id, &mut bytes).is_err() {
            panic!("Invalid identifier: {:?}", other)
        }

        Self(bytes.into())
    }
}

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

    pub fn current(system: &'a System) -> Self {
        let nonce = system.0.borrow_mut().free_id_nonce();

        Self::current_with_id(system, nonce)
    }

    pub fn current_with_id<I: Into<ProgramIdWrapper> + Clone + Debug>(
        system: &'a System,
        id: I,
    ) -> Self {
        Self::from_file_with_id(system, id, Self::wasm_path("wasm"))
    }

    pub fn current_opt(system: &'a System) -> Self {
        let nonce = system.0.borrow_mut().free_id_nonce();

        Self::current_opt_with_id(system, nonce)
    }

    pub fn current_opt_with_id<I: Into<ProgramIdWrapper> + Clone + Debug>(
        system: &'a System,
        id: I,
    ) -> Self {
        Self::from_file_with_id(system, id, Self::wasm_path("opt.wasm"))
    }

    pub fn mock<T: WasmProgram + 'static>(system: &'a System, mock: T) -> Self {
        let nonce = system.0.borrow_mut().free_id_nonce();

        Self::mock_with_id(system, nonce, mock)
    }

    pub fn mock_with_id<T: WasmProgram + 'static, I: Into<ProgramIdWrapper> + Clone + Debug>(
        system: &'a System,
        id: I,
        mock: T,
    ) -> Self {
        Self::program_with_id(system, id, InnerProgram::new_mock(mock))
    }

    pub fn from_file<P: AsRef<Path>>(system: &'a System, path: P) -> Self {
        let nonce = system.0.borrow_mut().free_id_nonce();

        Self::from_file_with_id(system, nonce, path)
    }

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
        let is_opt = filename.ends_with(".opt.wasm");

        let (opt_code, meta_code) = if !is_opt {
            let mut optimizer = Optimizer::new(path).expect("Failed to create optimizer");
            optimizer.insert_stack_and_export();
            let opt_code = optimizer
                .optimize()
                .expect("Failed to produce optimized binary");
            let meta_code = optimizer
                .metadata()
                .expect("Failed to produce metadata binary");
            (opt_code, Some(meta_code))
        } else {
            (
                fs::read(&path).unwrap_or_else(|_| panic!("Failed to read file {:?}", path)),
                None,
            )
        };

        Self::from_opt_and_meta_code_with_id(system, id, opt_code, meta_code)
    }

    pub fn from_opt_and_meta<P: AsRef<Path>, I: Into<ProgramIdWrapper> + Clone + Debug>(
        system: &'a System,
        optimized: P,
        metadata: P,
    ) -> Self {
        let nonce = system.0.borrow_mut().free_id_nonce();
        Self::from_opt_and_meta_with_id(system, nonce, optimized, metadata)
    }

    pub fn from_opt_and_meta_with_id<P: AsRef<Path>, I: Into<ProgramIdWrapper> + Clone + Debug>(
        system: &'a System,
        id: I,
        optimized: P,
        metadata: P,
    ) -> Self {
        let read_file = |path: P, ext| {
            let path = env::current_dir()
                .expect("Unable to get root directory of the project")
                .join(path)
                .clean();

            let filename = path.file_name().and_then(OsStr::to_str).unwrap_or_default();
            assert!(filename.ends_with(ext), "Wrong file extension: {}", ext);

            fs::read(&path).unwrap_or_else(|_| panic!("Failed to read file {:?}", path))
        };

        let opt_code = read_file(optimized, ".opt.wasm");
        let meta_code = read_file(metadata, ".meta.wasm");

        Self::from_opt_and_meta_code_with_id(system, id, opt_code, Some(meta_code))
    }

    pub fn from_opt_and_meta_code_with_id<I: Into<ProgramIdWrapper> + Clone + Debug>(
        system: &'a System,
        id: I,
        optimized: Vec<u8>,
        metadata: Option<Vec<u8>>,
    ) -> Self {
        let code = Code::try_new(optimized, 1, |_| ConstantCostRules::default())
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
        let program = CoreProgram::new(program_id, code);

        Self::program_with_id(
            system,
            id,
            InnerProgram::new(program, code_id, Default::default()),
        )
    }

    pub fn send<ID: Into<ProgramIdWrapper>, C: Codec>(&self, from: ID, payload: C) -> RunResult {
        self.send_with_value(from, payload, 0)
    }

    pub fn send_with_value<ID: Into<ProgramIdWrapper>, C: Codec>(
        &self,
        from: ID,
        payload: C,
        value: u128,
    ) -> RunResult {
        self.send_bytes_with_value(from, payload.encode(), value)
    }

    pub fn send_bytes<ID: Into<ProgramIdWrapper>, T: AsRef<[u8]>>(
        &self,
        from: ID,
        payload: T,
    ) -> RunResult {
        self.send_bytes_with_value(from, payload, 0)
    }

    pub fn send_bytes_with_value<ID: Into<ProgramIdWrapper>, T: AsRef<[u8]>>(
        &self,
        from: ID,
        payload: T,
        value: u128,
    ) -> RunResult {
        let mut system = self.manager.borrow_mut();

        let source = from.into().0;

        if !system.is_user(&source) {
            panic!("Sending messages allowed only from users id");
        }

        if 0 < value && value < crate::EXISTENTIAL_DEPOSIT {
            panic!(
                "Value greater than 0, but less than required existential deposit ({})",
                crate::EXISTENTIAL_DEPOSIT
            );
        }

        let message = Message::new(
            MessageId::generate_from_user(
                system.block_info.height,
                source,
                system.fetch_inc_message_nonce() as u128,
            ),
            source,
            self.id,
            payload.as_ref().to_vec(),
            Some(u64::MAX),
            value,
            None,
        );

        let (actor, _) = system.actors.get_mut(&self.id).expect("Can't fail");

        let kind = if let Actor::Uninitialized(id, _) = actor {
            if id.is_none() {
                *id = Some(message.id());
                DispatchKind::Init
            } else {
                DispatchKind::Handle
            }
        } else {
            DispatchKind::Handle
        };

        let dispatch = Dispatch::new(kind, message);

        system.run_dispatch(dispatch)
    }

    pub fn id(&self) -> ProgramId {
        self.id
    }

    pub fn meta_state<E: Encode, D: Decode>(&self, payload: E) -> Result<D> {
        D::decode(&mut self.meta_state_with_bytes(payload.encode())?.as_slice()).map_err(Into::into)
    }

    pub fn meta_state_with_bytes(&self, payload: impl AsRef<[u8]>) -> Result<Vec<u8>> {
        self.manager
            .borrow_mut()
            .call_meta(&self.id, Some(payload.as_ref().into()), "meta_state")
    }

    pub fn meta_state_empty<D: Decode>(&self) -> Result<D> {
        D::decode(&mut self.meta_state_empty_with_bytes()?.as_slice()).map_err(Into::into)
    }

    pub fn meta_state_empty_with_bytes(&self) -> Result<Vec<u8>> {
        self.manager
            .borrow_mut()
            .call_meta(&self.id, None, "meta_state")
    }

    pub fn mint(&mut self, value: Balance) {
        self.manager.borrow_mut().mint_to(&self.id(), value)
    }

    pub fn balance(&self) -> Balance {
        self.manager.borrow().balance_of(&self.id())
    }

    fn wasm_path(extension: &str) -> PathBuf {
        let current_dir = env::current_dir().expect("Unable to get current dir");
        let path_file = current_dir.join(".binpath");
        let path_bytes = fs::read(path_file).expect("Unable to read path bytes");
        let mut relative_path: PathBuf =
            String::from_utf8(path_bytes).expect("Invalid path").into();
        relative_path.set_extension(extension);
        current_dir.join(relative_path)
    }
}

pub fn calculate_program_id(code_hash: CodeId, salt: &[u8]) -> ProgramId {
    ProgramId::generate(code_hash, salt)
}

#[cfg(test)]
mod tests {
    use super::Program;
    use crate::{Log, System};

    #[test]
    fn test_handle_messages_to_failing_program() {
        let sys = System::new();
        sys.init_logger();

        let user_id = 100;

        let prog = Program::from_file(
            &sys,
            "../target/wasm32-unknown-unknown/release/demo_futures_unordered.wasm",
        );

        let init_msg_payload = String::from("InvalidInput");
        let run_result = prog.send(user_id, init_msg_payload);
        assert!(run_result.main_failed);

        let run_result = prog.send(user_id, String::from("should_be_skipped"));

        let expected_log = Log::error_builder(2).source(prog.id()).dest(user_id);

        assert!(!run_result.main_failed());
        assert!(run_result.contains(&expected_log));
    }

    #[test]
    fn simple_balance() {
        let sys = System::new();
        sys.init_logger();

        let user_id = 42;
        sys.mint_to(user_id, 5000);
        assert_eq!(sys.balance_of(user_id), 5000);

        let mut prog = Program::from_file(
            &sys,
            "../target/wasm32-unknown-unknown/release/demo_ping.wasm",
        );

        prog.mint(1000);
        assert_eq!(prog.balance(), 1000);

        prog.send_with_value(user_id, "init".to_string(), 500);
        assert_eq!(prog.balance(), 1500);
        assert_eq!(sys.balance_of(user_id), 4500);

        prog.send_with_value(user_id, "PING".to_string(), 1000);
        assert_eq!(prog.balance(), 2500);
        assert_eq!(sys.balance_of(user_id), 3500);
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
        sys.mint_to(sender0, 10000);
        sys.mint_to(sender1, 10000);
        sys.mint_to(sender2, 10000);

        let prog = Program::from_file(
            &sys,
            "../target/wasm32-unknown-unknown/release/demo_piggy_bank.wasm",
        );

        prog.send_bytes(receiver, b"init");
        assert_eq!(prog.balance(), 0);

        // Send values to the program
        prog.send_bytes_with_value(sender0, b"insert", 1000);
        assert_eq!(sys.balance_of(sender0), 9000);
        prog.send_bytes_with_value(sender1, b"insert", 2000);
        assert_eq!(sys.balance_of(sender1), 8000);
        prog.send_bytes_with_value(sender2, b"insert", 3000);
        assert_eq!(sys.balance_of(sender2), 7000);

        // Check program's balance
        assert_eq!(prog.balance(), 1000 + 2000 + 3000);

        // Request to smash the piggy bank and send the value to the receiver address
        prog.send_bytes(receiver, b"smash");
        sys.claim_value_from_mailbox(receiver);
        assert_eq!(sys.balance_of(receiver), 1000 + 2000 + 3000);

        // Check program's balance is empty
        assert_eq!(prog.balance(), 0);
    }
}
