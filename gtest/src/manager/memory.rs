// This file is part of Gear.
//
// Copyright (C) 2021-2024 Gear Technologies Inc.
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
        program_id: &ProgramId,
    ) -> Result<Vec<u8>> {
        let executable_actor_data = Actors::modify(*program_id, |actor| {
            if let Some(actor) = actor {
                Ok(actor.get_executable_actor_data())
            } else {
                Err(TestError::ActorNotFound(*program_id))
            }
        })?;

        if let Some((data, code, code_metadata)) = executable_actor_data {
            core_processor::informational::execute_for_reply::<Ext<LazyPagesNative>, _>(
                String::from("state"),
                code,
                code_metadata,
                Some(data.allocations),
                Some((*program_id, Default::default())),
                payload,
                GAS_ALLOWANCE,
                self.blocks_manager.get(),
            )
            .map_err(TestError::ReadStateError)
        } else if let Some(mut program_mock) = Actors::modify(*program_id, |actor| {
            actor.expect("Checked before").take_mock()
        }) {
            program_mock
                .state()
                .map_err(|err| TestError::ReadStateError(err.into()))
        } else {
            Err(TestError::ActorIsNotExecutable(*program_id))
        }
    }

    pub(crate) fn read_state_bytes_using_wasm(
        &mut self,
        payload: Vec<u8>,
        program_id: &ProgramId,
        fn_name: &str,
        wasm: Vec<u8>,
        args: Option<Vec<u8>>,
    ) -> Result<Vec<u8>> {
        let mapping_code = Code::try_new_mock_const_or_no_rules(
            wasm,
            true,
            TryNewCodeConfig::with_no_exports_check(),
        )
        .map_err(|_| TestError::Instrumentation)?;

        let (_, mapping_code, code_metadata) =
            CodeAndId::new(mapping_code).into_parts().0.into_parts();

        let mut mapping_code_payload = args.unwrap_or_default();
        mapping_code_payload.append(&mut self.read_state_bytes(payload, program_id)?);

        core_processor::informational::execute_for_reply::<Ext<LazyPagesNative>, _>(
            String::from(fn_name),
            mapping_code,
            code_metadata,
            None,
            None,
            mapping_code_payload,
            GAS_ALLOWANCE,
            self.blocks_manager.get(),
        )
        .map_err(TestError::ReadStateError)
    }

    pub(crate) fn read_memory_pages(&self, program_id: &ProgramId) -> BTreeMap<GearPage, PageBuf> {
        Actors::access(*program_id, |actor| {
            let program = match actor.unwrap_or_else(|| panic!("Actor id {program_id:?} not found"))
            {
                TestActor::Initialized(program) => program,
                TestActor::Uninitialized(_, program) => program.as_ref().unwrap(),
                TestActor::Dormant => panic!("Actor {program_id} isn't dormant"),
            };

            match program {
                Program::Genuine(program) => program.pages_data.clone(),
                Program::Mock(_) => panic!("Can't read memory of mock program"),
            }
        })
    }
}
