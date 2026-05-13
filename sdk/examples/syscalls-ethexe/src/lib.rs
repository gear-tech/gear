// This file is part of Gear.

// Copyright (C) 2021-2025 Gear Technologies Inc.
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

//! Mega syscall fuzz-testing contract for ethexe.
//!
//! This program exercises most syscalls available in ethexe by accepting
//! a sequence of [`FuzzCommand`]s encoded as the message payload. Each
//! command triggers one or more syscalls with fuzz-derived parameters.
//! On success the program replies with `b"ok"`.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::vec::Vec;
use parity_scale_codec::{Decode, Encode};

#[cfg(feature = "std")]
mod code {
    include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));
}

#[cfg(feature = "std")]
pub use code::WASM_BINARY_OPT as WASM_BINARY;

/// A single fuzz operation that exercises one or more ethexe-available syscalls.
#[derive(Debug, Clone, Encode, Decode)]
pub enum FuzzCommand {
    // ── Message info ──────────────────────────────────────────────────
    /// Read the incoming payload and verify its length matches `msg::size()`.
    CheckSize,
    /// Retrieve the current message id via `msg::id()`.
    CheckMessageId,
    /// Retrieve the program id via `exec::program_id()`.
    CheckProgramId,
    /// Retrieve the message source via `msg::source()`.
    CheckSource,
    /// Retrieve the attached value via `msg::value()`.
    CheckValue,

    // ── Environment info ──────────────────────────────────────────────
    /// Read `exec::block_height()`.
    CheckBlockHeight,
    /// Read `exec::block_timestamp()`.
    CheckBlockTimestamp,
    /// Read `exec::gas_available()`.
    CheckGasAvailable,
    /// Read `exec::value_available()`.
    CheckValueAvailable,
    /// Read `exec::env_vars()`.
    CheckEnvVars,

    // ── Sending messages ──────────────────────────────────────────────
    /// `msg::send(dest, payload, value)` — send a message to `dest`.
    SendMessage {
        dest: [u8; 32],
        payload: Vec<u8>,
        value: u128,
    },
    /// Build a message in parts via `send_init` / `send_push` / `send_commit`.
    SendRaw {
        dest: [u8; 32],
        payload: Vec<u8>,
    },
    /// Forward the current input to `dest` via `msg::send_input`.
    SendInput {
        dest: [u8; 32],
    },

    // ── Reply ─────────────────────────────────────────────────────────
    /// `msg::reply(payload, value)`.
    ReplyMessage {
        payload: Vec<u8>,
        value: u128,
    },
    /// Build a reply in parts: `reply_push` + `reply_commit`.
    ReplyRaw {
        payload: Vec<u8>,
    },
    /// Reply with the current input via `msg::reply_input`.
    ReplyInput,

    // ── Memory management ─────────────────────────────────────────────
    /// Call `alloc` for `pages` pages, then `free` the first allocated page.
    AllocAndFree {
        alloc_pages: u32,
    },
    /// Stress-test memory: allocate `count` pages, write a pattern, verify, free.
    MemStress {
        count: u32,
        pattern: u8,
    },
    /// Persistently append bytes to in-memory state and verify selected reads.
    ReadBigState {
        chunk_size: u32,
        repeat: u16,
    },

    /// Execute `exec::wait()` — pauses execution (terminates this handle call).
    /// Only the *last* command in a sequence should use this.
    WaitCmd,
    /// Execute `exec::wait_for(duration)`.
    WaitForCmd(u32),
    /// Execute `exec::wait_up_to(duration)`.
    WaitUpToCmd(u32),

    // ── Debug ─────────────────────────────────────────────────────────
    /// Call `gstd::debug!` with a message.
    DebugMessage(Vec<u8>),

    Noop,
}

/// Initialization payload: just an opaque blob the program stores.
#[derive(Debug, Clone, Encode, Decode)]
pub struct InitConfig {
    /// An optional "echo target" program id. If set, some send commands
    /// will default to sending here when dest is zero.
    pub echo_dest: Option<[u8; 32]>,
}

#[cfg(not(feature = "std"))]
mod wasm;
