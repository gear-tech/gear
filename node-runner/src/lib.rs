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

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub mod ext;
mod report;

use alloc::collections::VecDeque;
use primitive_types::H256;

use gear_core::{
    message::{Message, MessageId},
    program::ProgramId,
    storage::Storage,
};

pub use core_runner::{BlockInfo, Ext};
use core_runner::{CoreRunner, EntryPoint, ExecutionSettings};
use sp_std::collections::btree_map::BTreeMap;
use sp_std::prelude::*;

use crate::ext::*;
pub use crate::report::*;

/// Storage used for running node
pub type ExtStorage = Storage<ExtProgramStorage>;

pub fn get_program(id: H256) -> Result<gear_core::program::Program, Error> {
    if let Some(prog) = gear_common::get_program(id) {
        let persistent_pages = gear_common::get_program_pages(id, prog.persistent_pages);
        let code = gear_common::get_code(prog.code_hash).ok_or(Error::Runner)?;
        let id: gear_core::program::ProgramId = id.as_ref().into();
        let mut program =
            gear_core::program::Program::new(id, code, persistent_pages).expect("Can't fail");
        program.set_message_nonce(prog.nonce);
        return Ok(program);
    };

    Err(Error::Runner)
}

pub fn set_program(program: gear_core::program::Program) {
    let mut persistent_pages = BTreeMap::<u32, Vec<u8>>::new();

    for (key, value) in program.get_pages().iter() {
        persistent_pages.insert(key.raw(), value.to_vec());
    }

    let id = H256::from_slice(program.id().as_slice());

    let code_hash: H256 = sp_io::hashing::blake2_256(program.code()).into();

    let program = gear_common::Program {
        static_pages: program.static_pages(),
        nonce: program.message_nonce(),
        persistent_pages: persistent_pages.keys().copied().collect(),
        code_hash,
    };

    gear_common::set_program(id, program, persistent_pages);
}

pub fn process(
    message: gear_common::Message,
    block_info: BlockInfo,
) -> Result<ExecutionReport, Error> {
    let mut env = gear_backend_sandbox::SandboxEnvironment::<crate::Ext>::new();

    let entry = if message.reply.is_some() {
        EntryPoint::HandleReply
    } else {
        EntryPoint::Handle
    };

    let program = get_program(message.dest)?;

    let program_code = gear_core::gas::instrument(program.code()).map_err(|_| Error::Runner)?;

    let settings = ExecutionSettings::new(entry, block_info);

    let message: gear_core::message::Message = message.into();

    let res = CoreRunner::run(&mut env, program, message.into(), &program_code, settings);

    set_program(res.program.clone());

    Ok(ExecutionReport::from_run_result(res))
}

pub fn init_program(
    program_code: Vec<u8>,
    message: gear_common::Message,
    block_info: BlockInfo,
) -> Result<ExecutionReport, Error> {
    let mut env = gear_backend_sandbox::SandboxEnvironment::<crate::Ext>::new();

    let message: gear_core::message::Message = message.into();

    let program =
        gear_core::program::Program::new(message.dest(), program_code, Default::default())
            .map_err(|_| Error::Runner)?;

    let program_code = gear_core::gas::instrument(program.code()).map_err(|_| Error::Runner)?;

    let settings = ExecutionSettings::new(EntryPoint::Init, block_info);

    let res = CoreRunner::run(&mut env, program, message.into(), &program_code, settings);

    set_program(res.program.clone());

    Ok(ExecutionReport::from_run_result(res))
}

pub fn gas_spent(program_id: H256, payload: Vec<u8>, block_info: BlockInfo) -> Result<u64, Error> {
    let mut env = gear_backend_sandbox::SandboxEnvironment::<crate::Ext>::new();

    let message = Message {
        id: MessageId::from_slice(&gear_common::next_message_id(&payload)[..]),
        source: ProgramId::from(1),
        dest: ProgramId::from_slice(&program_id[..]),
        gas_limit: u64::MAX,
        payload: payload.into(),
        value: 0,
        reply: None,
    };

    let mut messages = VecDeque::from([message]);

    let mut total_gas_spent = 0;

    while let Some(message) = messages.pop_front() {
        let entry = if message.reply().is_some() {
            EntryPoint::HandleReply
        } else {
            EntryPoint::Handle
        };

        let settings = ExecutionSettings::new(entry, block_info);

        let program = get_program(H256::from_slice(message.dest().as_slice()))?;
        set_program(program.clone());

        let program_code = gear_core::gas::instrument(program.code()).map_err(|_| Error::Runner)?;

        let res = CoreRunner::run(&mut env, program, message.into(), &program_code, settings);

        if res.outcome.was_trap() {
            return Err(Error::Runner);
        }

        for msg in res.messages {
            let dest = H256::from_slice(msg.dest().as_slice());

            if gear_common::program_exists(dest) {
                messages.push_back(msg);
            }
        }

        total_gas_spent += res.gas_spent;
    }

    Ok(total_gas_spent)
}
