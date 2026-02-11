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

//! Random [`FuzzCommand`] generator for the mega-contract fuzz mode.

use demo_syscalls_ethexe::FuzzCommand;
use gprimitives::ActorId;
use rand::RngCore;

/// Total number of non-wait command variants we can generate.
const CMD_VARIANT_COUNT: u32 = 17;

/// Generate a random sequence of fuzz commands.
///
/// The generator avoids `Wait*` commands (which would cause the program to
/// suspend execution and stop processing further commands) unless explicitly
/// requested, keeping the contract alive for continuous fuzzing.
pub fn generate_fuzz_commands(
    rng: &mut impl RngCore,
    max_commands: usize,
    program_id: ActorId,
) -> Vec<FuzzCommand> {
    let count = 1 + (rng.next_u32() as usize % max_commands.max(1));
    let mut commands = Vec::with_capacity(count);

    for _ in 0..count {
        let cmd = generate_one(rng, program_id);
        commands.push(cmd);
    }

    commands
}

fn generate_one(rng: &mut impl RngCore, program_id: ActorId) -> FuzzCommand {
    match rng.next_u32() % CMD_VARIANT_COUNT {
        // ── Message info (5 variants) ──
        0 => FuzzCommand::CheckSize,
        1 => FuzzCommand::CheckMessageId,
        2 => FuzzCommand::CheckProgramId,
        3 => FuzzCommand::CheckSource,
        4 => FuzzCommand::CheckValue,

        // ── Env info (5 variants) ──
        5 => FuzzCommand::CheckBlockHeight,
        6 => FuzzCommand::CheckBlockTimestamp,
        7 => FuzzCommand::CheckGasAvailable,
        8 => FuzzCommand::CheckValueAvailable,
        9 => FuzzCommand::CheckEnvVars,

        // ── Sending (3 variants) ──
        10 => {
            // Send message back to self (the mega contract) or to source
            let dest = if rng.next_u32().is_multiple_of(2) {
                program_id.into()
            } else {
                [0u8; 32] // zero = falls back to msg::source()
            };
            let payload_len = rng.next_u32() as usize % 128;
            let payload = random_bytes(rng, payload_len);
            FuzzCommand::SendMessage {
                dest,
                payload,
                value: 0,
            }
        }
        11 => {
            let dest = program_id.into();
            let payload_len = rng.next_u32() as usize % 64;
            let payload = random_bytes(rng, payload_len);
            FuzzCommand::SendRaw { dest, payload }
        }
        12 => {
            let dest = program_id.into();
            FuzzCommand::SendInput { dest }
        }

        // ── Memory (2 variants) ──
        13 => {
            let pages = 1 + rng.next_u32() % 8;
            FuzzCommand::AllocAndFree { alloc_pages: pages }
        }
        14 => {
            let count = 1 + rng.next_u32() % 4;
            let pattern = rng.next_u32() as u8;
            FuzzCommand::MemStress { count, pattern }
        }

        // ── Debug ──
        15 => {
            let len = rng.next_u32() as usize % 64;
            let data = random_bytes(rng, len);
            FuzzCommand::DebugMessage(data)
        }

        // ── Noop ──
        16 => FuzzCommand::Noop,

        _ => FuzzCommand::Noop,
    }
}

fn random_bytes(rng: &mut impl RngCore, len: usize) -> Vec<u8> {
    let mut buf = vec![0u8; len];
    let mut i = 0;
    while i < len {
        let word = rng.next_u64().to_le_bytes();
        let take = (len - i).min(8);
        buf[i..i + take].copy_from_slice(&word[..take]);
        i += take;
    }
    buf
}
