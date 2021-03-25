use crate::Runtime;
use pallet_gear::data;
use gear_core::{
    message::Message,
    program::{ProgramId, Program},
};

use sp_core::H256;

pub fn queue_message(message: Message) {
    let message = data::Message {
        source: H256::from_slice(&message.source.as_slice()),
        dest: H256::from_slice(&message.dest.as_slice()),
        payload: message.payload.into_raw(),
    };

    pallet_gear::queue_message::<Runtime>(message)
}

pub fn dequeue_message() -> Option<Message> {
    pallet_gear::dequeue_message::<Runtime>()
        .map(|msg| {
            Message {
                source: ProgramId::from_slice(&msg.source[..]),
                dest: ProgramId::from_slice(&msg.dest[..]),
                payload: msg.payload.into(),
            }
        })
}

pub fn get_program(id: ProgramId) -> Option<Program> {
    pallet_gear::get_program::<Runtime>(H256::from_slice(id.as_slice()))
        .map(|prog| {
            Program::new(
                id,
                prog.static_pages,
                prog.code,
            )
        })
}

pub fn set_program(program: Program) {
    pallet_gear::set_program::<Runtime>(
        H256::from_slice(program.id().as_slice()),
        data::Program {
            static_pages: program.static_pages().to_vec(),
            code: program.code().to_vec(),
        }
    );
}

pub fn remove_program(id: ProgramId) {
    pallet_gear::remove_program::<Runtime>(H256::from_slice(id.as_slice()));
}

pub fn page_info(page: u32) -> Option<ProgramId> {
    pallet_gear::page_info::<Runtime>(page).map(|pid| ProgramId::from_slice(&pid[..]))
}

pub fn alloc(page: u32, pid: ProgramId) {
    pallet_gear::alloc::<Runtime>(page, H256::from_slice(pid.as_slice()));
}

pub fn dealloc(page: u32) {
    pallet_gear::dealloc::<Runtime>(page);
}
