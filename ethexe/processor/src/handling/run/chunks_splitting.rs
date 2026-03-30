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

// An alias introduced for better readability of the chunks splitting steps.
type Chunks = Vec<Vec<(ActorId, H256)>>;

// `prepare_execution_chunks` is not exactly sorting (sorting usually `n*log(n)` this one is `O(n)`),
// but rather partitioning into subsets (chunks) of programs with approximately similar queue sizes.
pub(super) fn prepare_execution_chunks(
    ctx: &mut impl RunContext,
    queue_type: MessageType,
) -> Chunks {
    let states = ctx.states(queue_type);
    let mut execution_chunks = ExecutionChunks::new(ctx.inner().chunk_size, states.len());

    for state in states {
        ctx.handle_chunk_data(&mut execution_chunks, state, queue_type);
    }

    execution_chunks.arrange_execution_chunks(ctx)
}

/// A helper  struct to bundle actor id, state hash and queue size together
/// for easier handling in chunk preparation.
pub struct ActorStateHashWithQueueSize {
    pub actor_id: ActorId,
    pub hash: H256,
    pub canonical_queue_size: usize,
    pub injected_queue_size: usize,
}

impl ActorStateHashWithQueueSize {
    pub fn new(actor_id: ActorId, state: StateHashWithQueueSize) -> Self {
        Self {
            actor_id,
            hash: state.hash,
            canonical_queue_size: state.canonical_queue_size as usize,
            injected_queue_size: state.injected_queue_size as usize,
        }
    }
}

/// A helper struct to manage execution chunks during their preparation.
pub struct ExecutionChunks {
    chunk_size: usize,
    chunks: Chunks,
}

impl ExecutionChunks {
    fn new(chunk_size: usize, tasks_len: usize) -> Self {
        let number_of_chunks = tasks_len.div_ceil(chunk_size);

        Self {
            chunk_size,
            chunks: vec![vec![]; number_of_chunks],
        }
    }

    /// Gets chunk index in chunks tasks queue.
    pub fn chunk_idx(&self, mq_size: usize) -> usize {
        // Simplest implementation of chunk partitioning '| 1 | 2 | 3 | 4 | ..'
        debug_assert_ne!(mq_size, 0);
        mq_size.min(self.chunks.len()) - 1
    }

    /// Inserts chunk execution data into the specified chunk index.
    pub fn insert_into(&mut self, idx: usize, actor_id: ActorId, hash: H256) {
        if let Some(chunk) = self.chunks.get_mut(idx) {
            chunk.push((actor_id, hash));
        } else {
            panic!(
                "Chunk index {idx} out of bounds: chunks number - {}",
                self.chunks.len()
            );
        }
    }

    /// Insert chunk execution data into the heaviest chunk (most prior, the last one).
    pub fn insert_into_heaviest(&mut self, actor_id: ActorId, hash: H256) {
        if let Some(chunk) = self.chunks.last_mut() {
            chunk.push((actor_id, hash));
        } else {
            panic!("Chunks are empty, cannot insert into heaviest chunk");
        }
    }

    /// Arranges execution chunks by merging uneven chunks and reversing their order,
    /// so the heaviest chunks are processed first.
    fn arrange_execution_chunks<R: RunContext>(self, ctx: &mut R) -> Chunks {
        self.chunks
            .into_iter()
            // Merge uneven chunks
            .flatten()
            // Repartition chunks in reverse order to ensure all chunks have an equal number of elements
            .rev()
            .chunks(self.chunk_size)
            // Convert into vector of vectors
            .into_iter()
            .map(|c| {
                c.into_iter()
                    // `check_task_no_run` function isn't used in a common execution, but it's used only for an overlaid one.
                    // The function is intended to nullify program queues only once before execution. If the queue wasn't nullified
                    // earlier the function will nullify it and skip spawning the job for the program queue as it's empty. If the queue
                    // was already nullified, the function will return `false` and the job will be spawned as usual.
                    // For more info, see impl of the [`OverlaidContext`].
                    .filter(|&(program_id, _)| !ctx.check_task_no_run(program_id))
                    .collect()
            })
            .collect()
    }
}
