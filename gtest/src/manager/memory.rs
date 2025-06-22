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

impl ExtManager {
    /// Call non-void meta function from actor stored in manager.
    /// Warning! This is a static call that doesn't change actors pages data.
    pub(crate) fn read_state_bytes(
        &mut self,
        payload: Vec<u8>,
        program_id: &ActorId,
    ) -> Result<Vec<u8>> {
        let executable_actor_data = Actors::modify(*program_id, |actor| {
            if let Some(actor) = actor {
                Ok(actor.executable_actor_data())
            } else {
                Err(TestError::ActorNotFound(*program_id))
            }
        })?;

        if let Some((data, code)) = executable_actor_data {
            core_processor::informational::execute_for_reply::<Ext<LazyPagesNative>, _>(
                String::from("state"),
                code,
                Some(data.allocations),
                Some((*program_id, Default::default())),
                payload,
                MAX_USER_GAS_LIMIT,
                self.blocks_manager.get(),
            )
            .map_err(TestError::ReadStateError)
        } else {
            Err(TestError::ActorIsNotExecutable(*program_id))
        }
    }
    pub(crate) fn read_memory_pages(&self, program_id: &ActorId) -> BTreeMap<GearPage, PageBuf> {
        Actors::access(*program_id, |actor| {
            let program = match actor.unwrap_or_else(|| panic!("Actor id {program_id:?} not found"))
            {
                _ => todo!(),
            };

            todo!()
        })
    }
}
