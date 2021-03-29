use gear_core::{
    runner::{Config, Runner},
    storage::Storage,
};

use crate::ext::*;

const MEMORY_KEY_PREFIX: &'static [u8] = b"g::memory";

pub type ExtRunner = Runner<ExtAllocationStorage, ExtMessageQueue, ExtProgramStorage>;

fn memory() -> Vec<u8> {
    sp_externalities::with_externalities(|ext| ext.storage(MEMORY_KEY_PREFIX))
        .expect("Called outside of externalities context")
        .unwrap_or_default()
}

pub fn set_memory(data: Vec<u8>) {
    sp_externalities::with_externalities(|ext| { ext.set_storage(MEMORY_KEY_PREFIX.to_vec(), data); })
        .expect("Called outside of externalities context");
}

pub fn new() -> ExtRunner {
    Runner::new(
        &Config::default(),
        Storage {
            allocation_storage: ExtAllocationStorage,
            message_queue: ExtMessageQueue::default(),
            program_storage: ExtProgramStorage,
        },
        &memory(),
    )
}
