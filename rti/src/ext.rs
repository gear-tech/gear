use std::collections::VecDeque;

use gear_core::{
    storage::{AllocationStorage, ProgramStorage, MessageQueue},
    program::{ProgramId, Program},
    memory::PageNumber,
    message::Message,
};

use codec::{Encode, Decode};

pub struct ExtAllocationStorage;
pub struct ExtProgramStorage;
pub struct ExtMessageQueue;

const ALLOCATION_KEY_PREFIX: &'static [u8] = b"g::alloc::";
const PROGRAM_KEY_PREFIX: &'static [u8] = b"g::program::";
const MESSAGES_KEY: &'static [u8] = b"g::messages";

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
            .map(|val| Program::decode(&mut &val[..]).expect("Values are always encoded correctly; DB corruption?"))
    }
}

impl MessageQueue for ExtMessageQueue {
    fn dequeue(&mut self) -> Option<Message> {
        sp_externalities::with_externalities(|ext|
            ext.storage(MESSAGES_KEY).map(|messages_val| {
                let mut messages = VecDeque::<Message>::decode(&mut &messages_val[..])
                    .expect("Values are always encoded correctly; DB corruption?");
                let next_message = messages.pop_front();
                ext.set_storage(MESSAGES_KEY.to_vec(), messages.encode());
                next_message
            }).unwrap_or_default()
        )
        .expect("Called outside of externalities context")
    }

    fn queue(&mut self, message: Message) {
        sp_externalities::with_externalities(|ext| {
            let mut messages = ext
                .storage(MESSAGES_KEY)
                .map(|val| VecDeque::<Message>::decode(&mut &val[..]).expect("Values are always encoded correctly; DB corruption?"))
                .unwrap_or_default();

            messages.push_back(message);

            ext.set_storage(MESSAGES_KEY.to_vec(), messages.encode());
        })
        .expect("Called outside of externalities context")
    }
}
