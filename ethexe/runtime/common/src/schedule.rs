use crate::{
    state::{ComplexStorage, ProgramState, Storage},
    InBlockTransitions,
};
use anyhow::Result;
use gear_core::{ids::ProgramId, tasks::TaskHandler};
use gprimitives::{ActorId, CodeId, MessageId, ReservationId, H256};

pub struct Handler<'a, S: Storage> {
    pub in_block_transitions: &'a mut InBlockTransitions,
    pub storage: &'a S,
}

impl<S: Storage> Handler<'_, S> {
    pub fn update_state(
        &mut self,
        program_id: ProgramId,
        f: impl FnOnce(&mut ProgramState) -> Result<()>,
    ) -> H256 {
        crate::update_state(self.in_block_transitions, self.storage, program_id, f)
    }

    pub fn update_state_with_storage(
        &mut self,
        program_id: ProgramId,
        f: impl FnOnce(&S, &mut ProgramState) -> Result<()>,
    ) -> H256 {
        crate::update_state_with_storage(self.in_block_transitions, self.storage, program_id, f)
    }
}

impl<'a, S: Storage> TaskHandler<ActorId> for Handler<'a, S> {
    fn remove_from_mailbox(&mut self, _user_id: ActorId, _message_id: MessageId) -> u64 {
        unimplemented!("TODO (breathx)")
    }
    fn send_dispatch(&mut self, _stashed_message_id: MessageId) -> u64 {
        unimplemented!("TODO (breathx)")
    }
    fn send_user_message(&mut self, _stashed_message_id: MessageId, _to_mailbox: bool) -> u64 {
        unimplemented!("TODO (breathx)")
    }
    // TODO (breathx): consider deprecation of delayed wakes + non-concrete waits.
    fn wake_message(&mut self, program_id: ProgramId, message_id: MessageId) -> u64 {
        log::trace!("Running scheduled task wake message {message_id} to {program_id}");

        // TODO (breathx): don't update state if not changed?
        self.update_state_with_storage(program_id, |storage, state| {
            let Some((dispatch, new_waitlist_hash)) = storage
                .modify_waitlist_if_changed(state.waitlist_hash.clone(), |waitlist| {
                    waitlist.remove(&message_id)
                })?
            else {
                return Ok(());
            };

            state.waitlist_hash = new_waitlist_hash;
            state.queue_hash = storage.modify_queue(state.queue_hash.clone(), |queue| {
                queue.push_back(dispatch);
            })?;

            Ok(())
        });

        0
    }

    /* Deprecated APIs */
    fn remove_from_waitlist(&mut self, _program_id: ProgramId, _message_id: MessageId) -> u64 {
        unreachable!("considering deprecation of it; use `wake_message` instead")
    }
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
