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

//! Crate provides support for wasm runtime.

#![no_std]

extern crate alloc;

pub mod funcs;

use alloc::{
    borrow::Cow,
    boxed::Box,
    collections::{BTreeMap, BTreeSet},
    vec::Vec,
};
use gear_core::{
    env::Ext,
    gas::GasAmount,
    memory::{Memory, PageBuf, PageNumber},
    message::{MessageId, OutgoingMessage, PayloadStore, ProgramInitMessage, ReplyMessage},
    program::{CodeHash, ProgramId},
};

pub const EXIT_TRAP_STR: &str = "exit";
pub const LEAVE_TRAP_STR: &str = "leave";
pub const WAIT_TRAP_STR: &str = "wait";

pub enum TerminationReason<'a> {
    Exit(ProgramId),
    Leave,
    Success,
    Trap {
        explanation: Option<&'static str>,
        description: Option<Cow<'a, str>>,
    },
    Wait,
}

pub struct ExtInfo {
    pub gas_amount: GasAmount,
    pub pages: BTreeSet<PageNumber>,
    pub accessed_pages: BTreeMap<PageNumber, Vec<u8>>,
    pub outgoing: Vec<OutgoingMessage>,
    pub init_messages: Vec<ProgramInitMessage>,
    pub reply: Option<ReplyMessage>,
    pub awakening: Vec<MessageId>,
    pub nonce: u64,
    pub program_candidates_data: BTreeMap<CodeHash, Vec<(ProgramId, MessageId)>>,
    pub payload_store: Option<PayloadStore>,

    pub trap_explanation: Option<&'static str>,

    pub exit_argument: Option<ProgramId>,
}

pub struct BackendReport<'a> {
    pub termination: TerminationReason<'a>,
    pub info: ExtInfo,
}

pub struct BackendError<'a> {
    pub gas_amount: GasAmount,
    pub reason: &'static str,
    pub description: Option<Cow<'a, str>>,
}

pub trait Environment<E: Ext + Into<ExtInfo> + 'static>: Default + Sized {
    /// Setup external environment, provide `ext`, set the beginning of the memory region
    /// to the `static_area` content after creatig instance.
    fn setup(
        &mut self,
        ext: E,
        binary: &[u8],
        memory_pages: &BTreeMap<PageNumber, Option<Box<PageBuf>>>,
        memory: &dyn Memory,
    ) -> Result<(), BackendError<'static>>;

    /// Run setuped instance starting at `entry_point` - wasm export function name.
    /// NOTE: external environment must be set up.
    fn execute(&mut self, entry_point: &str) -> Result<BackendReport, BackendError>;

    /// Create internal representation of wasm memory with size `total_pages`
    fn create_memory(&self, total_pages: u32) -> Result<Box<dyn Memory>, &'static str>;
}
