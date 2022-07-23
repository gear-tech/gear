// This file is part of Gear.

// Copyright (C) 2022 Gear Technologies Inc.
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

use super::*;
use codec::{Decode, Encode};
use common::{storage::*, Origin as _};
use frame_support::dispatch::DispatchResult;
use gear_core::{
    ids::{MessageId, ProgramId},
    memory::{PageBuf, PageNumber},
    message::StoredDispatch,
};
use primitive_types::H256;
use scale_info::TypeInfo;
use sp_runtime::SaturatedConversion;
use sp_std::{collections::btree_map::BTreeMap, convert::TryInto, vec::Vec};

#[derive(Clone, Debug, PartialEq, Eq, Decode, Encode, TypeInfo)]
pub(super) struct PausedProgram {
    program_id: ProgramId,
    program: common::ActiveProgram,
    pages_hash: H256,
    wait_list_hash: H256,
    waiting_init: Vec<MessageId>,
}

fn memory_pages_hash(pages: &BTreeMap<PageNumber, PageBuf>) -> H256 {
    pages.using_encoded(sp_io::hashing::blake2_256).into()
}

fn wait_list_hash(wait_list: &BTreeMap<MessageId, StoredDispatch>) -> H256 {
    wait_list.using_encoded(sp_io::hashing::blake2_256).into()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PauseError {
    ProgramNotFound,
    ProgramTerminated,
    InvalidPageDataSize,
}

impl<T: Config> pallet::Pallet<T> {
    pub fn pause_program(program_id: ProgramId) -> Result<(), PauseError> {
        let program =
            common::get_program(program_id.into_origin()).ok_or(PauseError::ProgramNotFound)?;
        let program: common::ActiveProgram = program
            .try_into()
            .map_err(|_| PauseError::ProgramTerminated)?;

        let pages_data = common::get_program_pages_data(program_id.into_origin(), &program)
            .map_err(|e| {
                log::error!("pause_program error: {}", e);
                PauseError::InvalidPageDataSize
            })?;

        // TODO: update gas limit in `ValueTree` here (issue #1022).
        let paused_program = PausedProgram {
            program_id,
            pages_hash: memory_pages_hash(&pages_data),
            program,
            wait_list_hash: wait_list_hash(
                &WaitlistOf::<T>::drain_key(program_id)
                    .map(|(d, ..)| (d.id(), d))
                    .collect(),
            ),
            waiting_init: common::waiting_init_take_messages(program_id),
        };

        // code shouldn't be removed
        // remove_program(program_id);
        sp_io::storage::clear_prefix(&common::pages_prefix(program_id.into_origin()), None);
        sp_io::storage::clear_prefix(&common::program_key(program_id.into_origin()), None);

        PausedPrograms::<T>::insert(program_id, paused_program);

        Self::deposit_event(Event::ProgramPaused(program_id));

        Ok(())
    }

    pub fn program_paused(id: ProgramId) -> bool {
        PausedPrograms::<T>::contains_key(id)
    }

    pub(super) fn resume_program_impl(
        program_id: ProgramId,
        memory_pages: BTreeMap<PageNumber, PageBuf>,
        wait_list: BTreeMap<MessageId, StoredDispatch>,
    ) -> DispatchResult {
        let paused_program =
            PausedPrograms::<T>::get(program_id).ok_or(Error::<T>::PausedProgramNotFound)?;

        if paused_program.pages_hash != memory_pages_hash(&memory_pages) {
            return Err(Error::<T>::WrongMemoryPages.into());
        }

        if paused_program.wait_list_hash != wait_list_hash(&wait_list) {
            return Err(Error::<T>::WrongWaitList.into());
        }

        PausedPrograms::<T>::remove(program_id);

        if let Err(err) = common::set_program_and_pages_data(
            program_id.into_origin(),
            paused_program.program,
            memory_pages,
        ) {
            log::error!("resume_program_impl error: {}", err);
            return Err(Error::<T>::NotAllocatedPageWithData.into());
        }

        wait_list.into_iter().for_each(|(_, d)| {
            WaitlistOf::<T>::insert(d, u64::MAX.saturated_into::<T::BlockNumber>())
                .expect("Duplicate message is wl");
        });
        sp_io::storage::set(
            &common::waiting_init_prefix(program_id),
            &paused_program.waiting_init.encode()[..],
        );

        Ok(())
    }
}
