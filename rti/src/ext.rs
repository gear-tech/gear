use gear_core::{
    storage::{AllocationStorage, ProgramStorage},
    program::{ProgramId, Program},
    memory::PageNumber,
};

use codec::{Encode, Decode};

pub struct ExtAllocationStorage;
pub struct ExtProgramStorage;

const ALLOCATION_KEY_PREFIX: &'static [u8] = b"g::alloc::";
const PROGRAM_KEY_PREFIX: &'static [u8] = b"g::program::";

pub fn allocation_key(id: PageNumber) -> Vec<u8> {
    let mut key = ALLOCATION_KEY_PREFIX.to_vec();
    id.raw().encode_to(&mut key);
    key
}

pub fn program_key(id: ProgramId) -> Vec<u8> {
    let mut key = PROGRAM_KEY_PREFIX.to_vec();
    id.encode_to(&mut key);
    key
}

impl AllocationStorage for ExtAllocationStorage {
    fn get(&self, id: PageNumber) -> Option<ProgramId> {
        sp_externalities::with_externalities(|ext| ext.storage(&allocation_key(id)))
            .expect("Called outside of externalities context")
            .map(|val| ProgramId::decode(&mut &val[..]).expect("Values are always encoded correctly").into())
    }

    fn remove(&mut self, id: PageNumber) -> Option<ProgramId> {
        sp_externalities::with_externalities(
            |ext| {
                let key = allocation_key(id);
                let prev = ext.storage(&key);
                ext.clear_storage(&key);
                prev
            })
            .expect("Called outside of externalities context")
            .map(|val| ProgramId::decode(&mut &val[..]).expect("Values are always encoded correctly; DB corruption?"))
    }

    fn set(&mut self, page: PageNumber, program: ProgramId) {
        sp_externalities::with_externalities(|ext| ext.set_storage(allocation_key(page), program.encode()))
            .expect("Called outside of externalities context")
    }

    fn clear(&mut self, _program_id: ProgramId) {
        unimplemented!()
    }
}

impl ProgramStorage for ExtProgramStorage {
    fn get(&self, id: ProgramId) -> Option<Program> {
        sp_externalities::with_externalities(|ext| ext.storage(&program_key(id)))
            .expect("Called outside of externalities context")
            .map(|val| Program::decode(&mut &val[..]).expect("Values are always encoded correctly; DB corruption?"))
    }

    fn set(&mut self, program: Program) -> Option<Program> {
        sp_externalities::with_externalities(
            |ext| {
                let key = program_key(program.id());
                let prev_val = ext.storage(&key);
                ext.set_storage(key, program.encode());
                prev_val
            })
            .expect("Called outside of externalities context")
            .map(|val| Program::decode(&mut &val[..]).expect("Values are always encoded correctly; DB corruption?"))
    }

    fn remove(&mut self, id: ProgramId) -> Option<Program> {
        sp_externalities::with_externalities(
            |ext| {
                let key = program_key(id);
                let prev_val = ext.storage(&key);
                ext.clear_storage(&key);
                prev_val
            })
            .expect("Called outside of externalities context")
            .map(|val| Program::decode(&mut &val[..]).expect("Values are always encoded correctly; DB corruption?"))    }
}
