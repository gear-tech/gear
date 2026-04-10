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

//! Applying state dump into a database.

use crate::CASDatabase;
use anyhow::{Context, Result};
use ethexe_common::{
    Announce, HashOf, ProgramStates, StateHashWithQueueSize,
    db::{AnnounceStorageRW, CodesStorageRW},
};
use ethexe_runtime_common::state::ProgramState;
use parity_scale_codec::Decode;

use super::StateDump;

impl StateDump {
    /// Apply the dump into the database under the given announce hash.
    ///
    /// Writes all CAS blobs, original codes, program-to-code mappings,
    /// and announce program states.
    pub fn apply(
        &self,
        storage: &(impl AnnounceStorageRW + CodesStorageRW + CASDatabase),
        announce_hash: HashOf<Announce>,
    ) -> Result<()> {
        let announce = storage
            .announce(announce_hash)
            .context("Provided announce must be set up in db")?;

        anyhow::ensure!(
            self.block_hash == announce.block_hash,
            "Dump block hash {} does not match announce block hash {}",
            self.block_hash,
            announce.block_hash
        );

        // Write all CAS blobs (includes code bytes and all state data).
        for blob in &self.blobs {
            storage.write(blob);
        }

        // Mark codes as valid (bytes already restored in CAS above).
        for code_id in &self.codes {
            storage.set_code_valid(*code_id, true);
        }

        // Write program-to-code mappings and build program states.
        let mut program_states = ProgramStates::new();
        for (program_id, (code_id, state_hash)) in &self.programs {
            storage.set_program_code_id(*program_id, *code_id);

            let state = ProgramState::decode(
                &mut &storage
                    .read(*state_hash)
                    .expect("state blob must be present after CAS restore")[..],
            )?;

            program_states.insert(
                *program_id,
                StateHashWithQueueSize {
                    hash: *state_hash,
                    canonical_queue_size: state.canonical_queue.cached_queue_size,
                    injected_queue_size: state.injected_queue.cached_queue_size,
                },
            );
        }

        // Write announce program states.
        storage.set_announce_program_states(announce_hash, program_states);

        Ok(())
    }
}
