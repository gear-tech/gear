use crate::{
    state::{
        ComplexStorage, Dispatch, MaybeHash, ProgramState, Storage, ValueWithExpiry,
        MAILBOX_VALIDITY,
    },
    InBlockTransitions,
};
use alloc::vec;
use anyhow::{anyhow, Result};
use ethexe_common::{
    db::{Rfm, ScheduledTask, Sd, Sum},
    router::{OutgoingMessage, ValueClaim},
};
use gear_core::{ids::ProgramId, message::ReplyMessage, tasks::TaskHandler};
use gear_core_errors::SuccessReplyReason;
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

impl<'a, S: Storage> TaskHandler<Rfm, Sd, Sum> for Handler<'a, S> {
    fn remove_from_mailbox(
        &mut self,
        (program_id, user_id): (ProgramId, ActorId),
        message_id: MessageId,
    ) -> u64 {
        let mut value_claim = None;

        self.update_state_with_storage(program_id, |storage, state| {
            let (
                ValueWithExpiry {
                    value: claimed_value,
                    expiry,
                },
                new_mailbox_hash,
            ) = storage
                .modify_mailbox_if_changed(state.mailbox_hash.clone(), |mailbox| {
                    let local_mailbox = mailbox.get_mut(&user_id)?;
                    let claimed_value = local_mailbox.remove(&message_id)?;

                    if local_mailbox.is_empty() {
                        mailbox.remove(&user_id);
                    }

                    Some(claimed_value)
                })?
                .ok_or_else(|| anyhow!("failed to find message in mailbox"))?;

            state.mailbox_hash = new_mailbox_hash;

            value_claim = Some(ValueClaim {
                message_id,
                destination: user_id,
                value: claimed_value,
            });

            let reply = Dispatch::reply(
                message_id,
                user_id,
                MaybeHash::Empty,
                0,
                SuccessReplyReason::Auto,
            );

            state.queue_hash =
                storage.modify_queue(state.queue_hash.clone(), |queue| queue.push_back(reply))?;

            Ok(())
        });

        if let Some(value_claim) = value_claim {
            self.in_block_transitions
                .modify_transition(program_id, |_state_hash, transition| {
                    transition.claims.push(value_claim)
                })
                .expect("can't be None");
        }

        0
    }

    fn send_dispatch(&mut self, (program_id, message_id): (ProgramId, MessageId)) -> u64 {
        self.update_state_with_storage(program_id, |storage, state| {
            let (
                ValueWithExpiry {
                    value: (dispatch, user_id),
                    ..
                },
                new_stash_hash,
            ) = storage
                .modify_stash_if_changed(state.stash_hash.clone(), |stash| {
                    stash.remove(&message_id)
                })?
                .ok_or_else(|| anyhow!("failed to find message in stash"))?;

            debug_assert!(user_id.is_none());

            state.stash_hash = new_stash_hash;
            state.queue_hash = storage.modify_queue(state.queue_hash.clone(), |queue| {
                queue.push_back(dispatch);
            })?;

            Ok(())
        });

        0
    }

    fn send_user_message(&mut self, stashed_message_id: MessageId, program_id: ProgramId) -> u64 {
        let mut dispatch_and_user = None;

        self.update_state_with_storage(program_id, |storage, state| {
            let (
                ValueWithExpiry {
                    value: (dispatch, user_id),
                    ..
                },
                new_stash_hash,
            ) = storage
                .modify_stash_if_changed(state.stash_hash.clone(), |stash| {
                    stash.remove(&stashed_message_id)
                })?
                .ok_or_else(|| anyhow!("failed to find message in stash"))?;

            state.stash_hash = new_stash_hash;
            dispatch_and_user = Some((
                dispatch,
                user_id.expect("the message intended to user contains no id"),
            ));

            Ok(())
        });

        if let Some((dispatch, user_id)) = dispatch_and_user {
            let expiry = self.in_block_transitions.schedule_task(
                MAILBOX_VALIDITY.try_into().expect("infallible"),
                ScheduledTask::RemoveFromMailbox((program_id, user_id), stashed_message_id),
            );

            self.update_state_with_storage(program_id, |storage, state| {
                state.mailbox_hash =
                    storage.modify_mailbox(state.mailbox_hash.clone(), |mailbox| {
                        let r = mailbox.entry(user_id).or_default().insert(
                            dispatch.id,
                            ValueWithExpiry {
                                value: dispatch.value,
                                expiry,
                            },
                        );

                        debug_assert!(r.is_none());
                    })?;

                Ok(())
            });

            let outgoing_message = dispatch.into_outgoing(self.storage, user_id);

            self.in_block_transitions
                .modify_transition(program_id, |_state_hash, transition| {
                    transition.messages.push(outgoing_message)
                })
                .expect("must be")
        }

        0
    }

    // TODO (breathx): consider deprecation of delayed wakes + non-concrete waits.
    fn wake_message(&mut self, program_id: ProgramId, message_id: MessageId) -> u64 {
        log::trace!("Running scheduled task wake message {message_id} to {program_id}");

        self.update_state_with_storage(program_id, |storage, state| {
            let (
                ValueWithExpiry {
                    value: dispatch, ..
                },
                new_waitlist_hash,
            ) = storage
                .modify_waitlist_if_changed(state.waitlist_hash.clone(), |waitlist| {
                    waitlist.remove(&message_id)
                })?
                .ok_or_else(|| anyhow!("failed to find message in waitlist"))?;

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
