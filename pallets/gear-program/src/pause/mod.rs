use super::*;
use codec::{Decode, Encode};
use common::{self, QueuedDispatch};
use scale_info::TypeInfo;
use sp_std::collections::btree_map::BTreeMap;
use frame_support::storage::PrefixIterator;

#[derive(Clone, Debug, PartialEq, Decode, Encode, TypeInfo)]
pub(super) struct PausedProgram {
    program_id: H256,
    program: common::ActiveProgram,
    pages_hash: H256,
    wait_list: Vec<QueuedDispatch>,
    waiting_init: Vec<H256>,
}

fn decode_dispatch_tuple(_key: &[u8], value: &[u8]) -> Result<(QueuedDispatch, u32), codec::Error> {
    <(QueuedDispatch, u32)>::decode(&mut &*value)
}

fn memory_pages_hash(pages: &BTreeMap<u32, Vec<u8>>) -> H256 {
    pages.using_encoded(sp_io::hashing::blake2_256).into()
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PauseError {
    ProgramNotFound,
    ProgramTerminated,
}

impl<T: Config> pallet::Pallet<T> {
    pub fn pause_program(program_id: H256) -> Result<(), PauseError> {
        let program = common::get_program(program_id).ok_or(PauseError::ProgramNotFound)?;
        let program: common::ActiveProgram = program
            .try_into()
            .map_err(|_| PauseError::ProgramTerminated)?;

        let prefix = common::wait_prefix(program_id);
        let previous_key = prefix.clone();

        let paused_program = PausedProgram {
            program_id,
            pages_hash: memory_pages_hash(
                &common::get_program_pages(program_id, program.persistent_pages.clone())
                    .expect("pause_program: active program exists, therefore pages do"),
            ),
            program,
            wait_list: PrefixIterator::<_, ()>::new(prefix, previous_key, decode_dispatch_tuple)
                .drain()
                .map(|(d, _)| d)
                .collect(),
            waiting_init: common::waiting_init_take_messages(program_id),
        };

        // code shouldn't be removed
        // remove_program(program_id);
        sp_io::storage::clear_prefix(&common::pages_prefix(program_id), None);
        sp_io::storage::clear_prefix(&common::program_key(program_id), None);

        PausedPrograms::<T>::insert(program_id, paused_program);

        Ok(())
    }

    pub fn paused_program_exists(id: H256) -> bool {
        PausedPrograms::<T>::contains_key(id)
    }
}
