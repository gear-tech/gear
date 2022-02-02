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

use alloc::{boxed::Box, collections::BTreeMap, string::String, vec::Vec};
use gear_core::{
    env::Ext,
    gas::GasAmount,
    memory::{Memory, PageBuf, PageNumber},
    message::{MessageId, OutgoingMessage, ReplyMessage},
};

pub const WAIT_TRAP_STR: &str = "wait";
pub const EXIT_TRAP_STR: &str = "exit";

pub enum TerminationReason {
    Success,
    Trap {
        explanation: Option<&'static str>,
        description: Option<String>,
    },
    Manual {
        wait: bool,
    },
}

pub struct ExtInfo {
    pub gas_amount: GasAmount,
    pub pages: BTreeMap<PageNumber, Vec<u8>>,
    pub outgoing: Vec<OutgoingMessage>,
    pub reply: Option<ReplyMessage>,
    pub awakening: Vec<MessageId>,
    pub nonce: u64,

    pub trap_explanation: Option<&'static str>,
}

pub struct ExecutionReport {
    pub termination: TerminationReason,
    pub info: ExtInfo,
}

pub struct ExecutionFail {
    pub reason: &'static str,
    pub gas_amount: GasAmount,
    pub description: Option<String>,
}

pub trait Environment<E: Ext + Into<ExtInfo> + 'static>: Default + Sized {
    fn setup_and_execute(
        &mut self,
        ext: E,
        binary: &[u8],
        memory_pages: &BTreeMap<PageNumber, Box<PageBuf>>,
        memory: &dyn Memory,
        entry_point: &str,
    ) -> Result<ExecutionReport, ExecutionFail>;

    fn create_memory(&self, total_pages: u32) -> Result<Box<dyn Memory>, &'static str>;
}
