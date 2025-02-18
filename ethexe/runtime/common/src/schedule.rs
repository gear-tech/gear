use crate::{
    state::{
        Dispatch, DispatchStash, Mailbox, PayloadLookup, Storage, ValueWithExpiry, Waitlist,
        MAILBOX_VALIDITY,
    },
    TransitionController,
};
use ethexe_common::{
    db::{Rfm, Schedule, ScheduledTask, Sd, Sum},
    gear::{Origin, ValueClaim},
};
use gear_core::{ids::ProgramId, tasks::TaskHandler};
use gear_core_errors::SuccessReplyReason;
use gprimitives::{ActorId, CodeId, MessageId, ReservationId};

pub struct Handler<'a, S: Storage> {
    pub controller: TransitionController<'a, S>,
}

impl<S: Storage> TaskHandler<Rfm, Sd, Sum> for Handler<'_, S> {
    fn remove_from_mailbox(
        &mut self,
        (program_id, user_id): (ProgramId, ActorId),
        message_id: MessageId,
    ) -> u64 {
        self.controller
            .update_state(program_id, |state, storage, transitions| {
                let ValueWithExpiry { value, .. } =
                    state.mailbox_hash.modify_mailbox(storage, |mailbox| {
                        mailbox
                            .remove(user_id, message_id)
                            .expect("failed to find message in mailbox")
                    });

                transitions.modify_transition(program_id, |transition| {
                    transition.claims.push(ValueClaim {
                        message_id,
                        destination: user_id,
                        value,
                    })
                });

                let reply = Dispatch::reply(
                    message_id,
                    user_id,
                    PayloadLookup::empty(),
                    0,
                    SuccessReplyReason::Auto,
                    // TODO(rmasl): use the actual origin (https://github.com/gear-tech/gear/pull/4460)
                    Origin::Ethereum,
                );

                state
                    .queue_hash
                    .modify_queue(storage, |queue| queue.queue(reply));
            });

        0
    }

    fn send_dispatch(&mut self, (program_id, message_id): (ProgramId, MessageId)) -> u64 {
        self.controller
            .update_state(program_id, |state, storage, _| {
                state.queue_hash.modify_queue(storage, |queue| {
                    let dispatch = state
                        .stash_hash
                        .modify_stash(storage, |stash| stash.remove_to_program(&message_id));

                    queue.queue(dispatch);
                });
            });

        0
    }

    fn send_user_message(&mut self, stashed_message_id: MessageId, program_id: ProgramId) -> u64 {
        self.controller
            .update_state(program_id, |state, storage, transitions| {
                let (dispatch, user_id) = state
                    .stash_hash
                    .modify_stash(storage, |stash| stash.remove_to_user(&stashed_message_id));

                let expiry = transitions.schedule_task(
                    MAILBOX_VALIDITY.try_into().expect("infallible"),
                    ScheduledTask::RemoveFromMailbox((program_id, user_id), stashed_message_id),
                );

                state.mailbox_hash.modify_mailbox(storage, |mailbox| {
                    mailbox.add(user_id, stashed_message_id, dispatch.value, expiry);
                });

                transitions.modify_transition(program_id, |transition| {
                    transition
                        .messages
                        .push(dispatch.into_message(storage, user_id))
                })
            });

        0
    }

    // TODO (breathx): consider deprecation of delayed wakes + non-concrete waits.
    fn wake_message(&mut self, program_id: ProgramId, message_id: MessageId) -> u64 {
        log::trace!("Running scheduled task wake message {message_id} to {program_id}");

        self.controller
            .update_state(program_id, |state, storage, _| {
                let ValueWithExpiry {
                    value: dispatch, ..
                } = state.waitlist_hash.modify_waitlist(storage, |waitlist| {
                    waitlist
                        .wake(&message_id)
                        .expect("failed to find message in waitlist")
                });

                state.queue_hash.modify_queue(storage, |queue| {
                    queue.queue(dispatch);
                })
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

/// A [`Schedule`] restorer.
///
/// Used primary for fast sync and tests
pub struct Restorer {
    current_block: u32,
    schedule: Schedule,
}

impl Restorer {
    /// Creates restorer.
    ///
    /// A current block required to detect whether a value expired or not
    pub fn new(current_block: u32) -> Self {
        Self {
            current_block,
            schedule: Default::default(),
        }
    }

    pub fn waitlist(&mut self, program_id: ActorId, waitlist: &Waitlist) {
        for (&message_id, &ValueWithExpiry { value: _, expiry }) in waitlist.as_ref() {
            if expiry <= self.current_block {
                continue;
            }

            self.schedule
                .entry(expiry)
                .or_default()
                .insert(ScheduledTask::WakeMessage(program_id, message_id));
        }
    }

    pub fn mailbox(&mut self, program_id: ActorId, mailbox: &Mailbox) {
        for (&user_id, user_mailbox) in mailbox.as_ref() {
            for (&message_id, &ValueWithExpiry { value: _, expiry }) in user_mailbox {
                if expiry <= self.current_block {
                    continue;
                }

                self.schedule
                    .entry(expiry)
                    .or_default()
                    .insert(ScheduledTask::RemoveFromMailbox(
                        (program_id, user_id),
                        message_id,
                    ));
            }
        }
    }

    pub fn stash(&mut self, program_id: ActorId, stash: &DispatchStash) {
        for (
            &message_id,
            &ValueWithExpiry {
                value: (ref dispatch, user_id),
                expiry,
            },
        ) in stash.as_ref()
        {
            debug_assert_eq!(message_id, dispatch.id);
            debug_assert_eq!(program_id, dispatch.source);

            if expiry <= self.current_block {
                continue;
            }

            let task = if user_id.is_some() {
                ScheduledTask::SendUserMessage {
                    message_id,
                    to_mailbox: program_id,
                }
            } else {
                ScheduledTask::SendDispatch((program_id, message_id))
            };

            self.schedule.entry(expiry).or_default().insert(task);
        }
    }

    pub fn build(self) -> Schedule {
        self.schedule
    }
}
