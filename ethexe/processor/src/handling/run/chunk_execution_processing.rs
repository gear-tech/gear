// This file is part of Gear.
//
// Copyright (C) 2024-2025 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use super::*;
use super::chunk_execution_spawn::ChunkItemOutput;

// Aliases introduced for better readability of the chunk journals processing steps.
type ProgramChunkJournals = (ActorId, ChunkJournals);
type ChunkJournals = Vec<ExtendedJournal>;
type ExtendedJournal = (Vec<JournalNote>, MessageType, bool);

/// Output of the chunk journals processing step.
///
/// Chunk journals processing is actually a loop, which can break early.
/// The early break must also stop other steps of the caller chunk processing
/// function. So to expose the logic in a clear way, the enum is introduced.
pub enum ChunkJournalsProcessingOutput {
    Processed,
    EarlyBreak,
}

/// Collects journals from all executed programs in the chunk.
///
/// The [`chunk_execution_spawn::spawn_chunk_execution`] step spawns tasks for each program in the chunk.
/// The loop in the functions handles the output of each task:
/// - modifies the state by setting a new state hash calculated by the [`ethexe_runtime_common::RuntimeJournalHandler`]
/// - collects journals for later processing
/// - tracks the maximum gas spent among all programs in the chunk
///
/// Due to the nature of the parallel program queues execution (see [`chunk_execution_spawn::spawn_chunk_execution`] gas allowance clarifications),
/// the actual gas allowance spent is actually the maximum among all programs in the chunk, not the sum.
pub fn collect_chunk_journals(
    ctx: &mut impl RunContext,
    chunk_outputs: Vec<ChunkItemOutput>,
) -> (Vec<ProgramChunkJournals>, u64) {
    let mut max_gas_spent_in_chunk = 0u64;

    let chunk_journals = chunk_outputs
        .into_iter()
        .map(
            |(program_id, new_state_hash, program_journals, gas_spent)| {
                // Handle state updates that occurred during journal processing within the runtime (allocations, pages).
                // This should happen before processing the journal notes because `send_dispatch` from another program can modify the state.
                ctx.inner_mut().transitions.modify(program_id, |state, _| {
                    state.hash = new_state_hash;
                });

                max_gas_spent_in_chunk = max_gas_spent_in_chunk.max(gas_spent);

                (program_id, program_journals)
            },
        )
        .collect();

    (chunk_journals, max_gas_spent_in_chunk)
}

/// Processes journal of each program in the chunk.
///
/// The processing is done with [`ethexe_runtime_common::JournalHandler`], which actually sends messages
/// generated after executing program queues.
///
/// The journals are processed sequentially in the order of programs in the chunk.
///
/// The `early_break` closure is intended for overlaid execution mode. The closure is intended to
/// nullify queues of receiver programs (if not nullified) until the expected reply is found.
/// If it's found, no nullification is done and the processing breaks early.
pub fn process_chunk_execution_journals(
    ctx: &mut impl RunContext,
    chunk_journals: Vec<ProgramChunkJournals>,
) -> ChunkJournalsProcessingOutput {
    for (program_id, program_journals) in chunk_journals {
        for (journal, message_type, call_reply) in program_journals {
            let break_flag = ctx.break_early(&journal);

            let mut journal_handler = ctx.journal_handler(program_id, message_type, call_reply);
            core_processor::handle_journal(journal, &mut journal_handler);

            if break_flag {
                return ChunkJournalsProcessingOutput::EarlyBreak;
            }
        }
    }

    ChunkJournalsProcessingOutput::Processed
}
