// This file is part of Gear.
//
// Copyright (C) 2021-2025 Gear Technologies Inc.
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

// todo [sab] state reading for mock

impl ExtManager {
    /// Call non-void meta function from actor stored in manager.
    /// Warning! This is a static call that doesn't change actors pages data.
    pub(crate) fn read_state_bytes(
        &mut self,
        payload: Vec<u8>,
        program_id: ActorId,
    ) -> Result<Vec<u8>> {
        let allocations = ProgramsStorageManager::allocations(program_id);
        let code_id = ProgramsStorageManager::access_program(program_id, |program| {
            program.and_then(|p| {
                if let Program::Active(ActiveProgram { code_id, .. }) = p {
                    Some(code_id.cast())
                } else {
                    None
                }
            })
        })
        .ok_or(TestError::ActorNotFound(program_id))?;
        let code_metadata = self
            .code_metadata(code_id)
            .cloned()
            .ok_or(TestError::ActorNotFound(program_id))?;
        let instrumented_code = self
            .instrumented_code(code_id)
            .cloned()
            .ok_or(TestError::ActorNotFound(program_id))?;
        core_processor::informational::execute_for_reply::<Ext<LazyPagesNative>, _>(
            String::from("state"),
            instrumented_code,
            code_metadata,
            allocations,
            Some((program_id, Default::default())),
            payload,
            MAX_USER_GAS_LIMIT,
            self.blocks_manager.get(),
        )
        .map_err(TestError::ReadStateError)
    }

    pub(crate) fn read_memory_pages(&self, program_id: ActorId) -> BTreeMap<GearPage, PageBuf> {
        ProgramsStorageManager::program_pages(program_id)
    }
}
