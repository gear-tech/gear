use gear_core::{
    storage::AllocationStorage,
    program::ProgramId,
    memory::PageNumber,
};

use codec::{Encode, Decode};

pub struct ExtAllocationStorage;

const ALLOCATION_KEY_PREFIX: &'static [u8] = b"g::alloc::";

pub fn allocation_key(id: PageNumber) -> Vec<u8> {
    let mut key = ALLOCATION_KEY_PREFIX.to_vec();
    id.raw().encode_to(&mut key);
    key
}

pub fn program_value(pid: ProgramId) -> Vec<u8> {
    let mut key = Vec::new();
    pid.0.encode_to(&mut key);
    key
}

impl AllocationStorage for ExtAllocationStorage {
    fn get(&self, id: PageNumber) -> Option<ProgramId> {
        sp_externalities::with_externalities(|ext| ext.storage(&allocation_key(id)))
            .expect("Called outside of externalities context")
            .map(|val| u64::decode(&mut &val[..]).expect("Values are always encoded correctly").into())
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
            .map(|val| u64::decode(&mut &val[..]).expect("Values are always encoded correctly").into())
    }

    fn set(&mut self, page: PageNumber, program: ProgramId) {
        sp_externalities::with_externalities(|ext| ext.set_storage(allocation_key(page), program_value(program)))
            .expect("Called outside of externalities context")
    }

    fn clear(&mut self, _program_id: ProgramId) {
        unimplemented!()
    }
}
