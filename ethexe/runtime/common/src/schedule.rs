use gear_core::{ids::ProgramId, tasks::TaskHandler};
use gprimitives::{ActorId, CodeId, MessageId, ReservationId};

use crate::{state::Storage, InBlockTransitions};

pub struct Handler<'a, S: Storage> {
    pub in_block_transitions: &'a mut InBlockTransitions,
    pub storage: &'a S,
}

impl<'a, S: Storage> TaskHandler<ActorId> for Handler<'a, S> {
    fn remove_from_mailbox(&mut self, _user_id: ActorId, _message_id: MessageId) -> u64 {
        unimplemented!("TODO (breathx)")
    }
    fn remove_from_waitlist(&mut self, _program_id: ProgramId, _message_id: MessageId) -> u64 {
        unimplemented!("TODO (breathx)")
    }
    fn send_dispatch(&mut self, _stashed_message_id: MessageId) -> u64 {
        unimplemented!("TODO (breathx)")
    }
    fn send_user_message(&mut self, _stashed_message_id: MessageId, _to_mailbox: bool) -> u64 {
        unimplemented!("TODO (breathx)")
    }
    fn wake_message(&mut self, _program_id: ProgramId, _message_id: MessageId) -> u64 {
        // TODO (breathx): consider deprecation of delayed wakes + non-concrete waits.
        unimplemented!("TODO (breathx)")
    }

    /* Deprecated APIs */
    fn pause_program(&mut self, _: ProgramId) -> u64 {
        unreachable!("deprecated")
    }
    fn remove_code(&mut self, _: CodeId) -> u64 {
        unreachable!("deprecated")
    }
    fn remove_gas_reservation(&mut self, _: ProgramId, _: ReservationId) -> u64 {
        unreachable!("deprecated")
    }
    fn remove_paused_program(&mut self, _: ProgramId) -> u64 {
        unreachable!("deprecated")
    }
    fn remove_resume_session(&mut self, _: u32) -> u64 {
        unreachable!("deprecated")
    }
}
