use crate::{
    TransitionController,
    state::{
        Dispatch, DispatchStash, Expiring, MAILBOX_VALIDITY, MailboxMessage, ModifiableStorage,
        PayloadLookup, ProgramState, QueriableStorage, Storage, UserMailbox, Waitlist,
    },
};
use alloc::collections::{BTreeMap, BTreeSet};
use anyhow::Context;
use ethexe_common::{ProgramStates, Rfm, Schedule, ScheduledTask, Sd, Sum, gear::ValueClaim};
use gear_core::tasks::TaskHandler;
use gear_core_errors::SuccessReplyReason;
use gprimitives::{ActorId, H256, MessageId, ReservationId};

pub struct Handler<'a, S: Storage> {
    pub controller: TransitionController<'a, S>,
}

impl<S: Storage> TaskHandler<Rfm, Sd, Sum> for Handler<'_, S> {
    fn remove_from_mailbox(
        &mut self,
        (program_id, user_id): (ActorId, ActorId),
        message_id: MessageId,
    ) -> u64 {
        self.controller
            .update_state(program_id, |state, storage, transitions| {
                let Expiring {
                    value: MailboxMessage { value, origin, .. },
                    ..
                } = storage.modify(&mut state.mailbox_hash, |mailbox| {
                    mailbox
                        .remove_and_store_user_mailbox(storage, user_id, message_id)
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
                    origin,
                    false,
                );

                let queue = state.queue_from_origin(origin);
                queue.modify_queue(storage, |queue| queue.queue(reply));
            });

        0
    }

    fn send_dispatch(&mut self, (program_id, message_id): (ActorId, MessageId)) -> u64 {
        self.controller
            .update_state(program_id, |state, storage, _| {
                let dispatch = storage.modify(&mut state.stash_hash, |stash| {
                    stash.remove_to_program(&message_id)
                });

                let queue = state.queue_from_origin(dispatch.origin);
                queue.modify_queue(storage, |queue| {
                    queue.queue(dispatch);
                });
            });

        0
    }

    fn send_user_message(&mut self, stashed_message_id: MessageId, program_id: ActorId) -> u64 {
        self.controller
            .update_state(program_id, |state, storage, transitions| {
                let (dispatch, user_id) = storage.modify(&mut state.stash_hash, |stash| {
                    stash.remove_to_user(&stashed_message_id)
                });

                let expiry = transitions.schedule_task(
                    MAILBOX_VALIDITY.try_into().expect("infallible"),
                    ScheduledTask::RemoveFromMailbox((program_id, user_id), stashed_message_id),
                );

                storage.modify(&mut state.mailbox_hash, |mailbox| {
                    mailbox.add_and_store_user_mailbox(
                        storage,
                        user_id,
                        stashed_message_id,
                        dispatch.clone().into(),
                        expiry,
                    );
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
    fn wake_message(&mut self, program_id: ActorId, message_id: MessageId) -> u64 {
        log::trace!("Running scheduled task wake message {message_id} to {program_id}");

        self.controller
            .update_state(program_id, |state, storage, _| {
                let Expiring {
                    value: dispatch, ..
                } = storage.modify(&mut state.waitlist_hash, |waitlist| {
                    waitlist
                        .wake(&message_id)
                        .expect("failed to find message in waitlist")
                });

                let queue = state.queue_from_origin(dispatch.origin);
                queue.modify_queue(storage, |queue| {
                    queue.queue(dispatch);
                })
            });

        0
    }

    /* Deprecated APIs */
    fn remove_from_waitlist(&mut self, _program_id: ActorId, _message_id: MessageId) -> u64 {
        unreachable!("considering deprecation of it; use `wake_message` instead")
    }
    fn remove_gas_reservation(&mut self, _: ActorId, _: ReservationId) -> u64 {
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
    /// A current block is required to detect whether a value expired or not
    pub fn new(current_block: u32) -> Self {
        Self {
            current_block,
            schedule: Default::default(),
        }
    }

    /// Creates a restorer from storage.
    ///
    /// Tries to fully restore schedule
    pub fn from_storage<T: Storage>(
        storage: &T,
        program_states: &ProgramStates,
        current_block: u32,
    ) -> anyhow::Result<Self> {
        let program_states: BTreeMap<H256, BTreeSet<ActorId>> =
            program_states
                .iter()
                .fold(BTreeMap::new(), |mut acc, (&program_id, state)| {
                    acc.entry(state.hash).or_default().insert(program_id);
                    acc
                });

        let mut restorer = Self::new(current_block);

        for (hash, program_ids) in program_states {
            let program_state = storage
                .program_state(hash)
                .context("failed to read ['Waitlist'] from storage by hash")?;
            let ProgramState {
                waitlist_hash,
                stash_hash,
                mailbox_hash,
                ..
            } = program_state;

            if let Ok(waitlist) = storage.query(&waitlist_hash) {
                for &program_id in &program_ids {
                    restorer.waitlist(program_id, &waitlist);
                }
            }

            if let Ok(stash) = storage.query(&stash_hash) {
                for &program_id in &program_ids {
                    restorer.stash(program_id, &stash);
                }
            }

            if let Ok(mailbox) = storage.query(&mailbox_hash) {
                for (&user_id, &user_mailbox) in mailbox.as_ref() {
                    let user_mailbox = storage
                        .user_mailbox(user_mailbox)
                        .context("failed to read ['UserMailbox'] from storage by hash")?;

                    for &program_id in &program_ids {
                        restorer.user_mailbox(program_id, user_id, &user_mailbox)
                    }
                }
            }
        }

        Ok(restorer)
    }

    pub fn waitlist(&mut self, program_id: ActorId, waitlist: &Waitlist) {
        for (
            &message_id,
            &Expiring {
                value: ref dispatch,
                expiry,
            },
        ) in waitlist.as_ref()
        {
            if expiry <= self.current_block {
                continue;
            }

            debug_assert_eq!(message_id, dispatch.id);

            self.schedule
                .entry(expiry)
                .or_default()
                .insert(ScheduledTask::WakeMessage(program_id, message_id));
        }
    }

    pub fn user_mailbox(
        &mut self,
        program_id: ActorId,
        user_id: ActorId,
        user_mailbox: &UserMailbox,
    ) {
        for (&message_id, &Expiring { value: _, expiry }) in user_mailbox.as_ref() {
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

    pub fn stash(&mut self, program_id: ActorId, stash: &DispatchStash) {
        for (
            &message_id,
            &Expiring {
                value: (ref dispatch, user_id),
                expiry,
            },
        ) in stash.as_ref()
        {
            debug_assert_eq!(message_id, dispatch.id);

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

    pub fn restore(self) -> Schedule {
        self.schedule
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{Mailbox, MemStorage};
    use ethexe_common::gear::Origin;
    use gear_core::buffer::Payload;
    use std::collections::{BTreeMap, BTreeSet};

    #[test]
    fn restorer_waitlist() {
        let program_id = ActorId::from(1);

        let dispatch = Dispatch::reply(
            MessageId::from(456),
            ActorId::from(789),
            PayloadLookup::Direct(Payload::repeat(0xfe)),
            0xffffff,
            SuccessReplyReason::Auto,
            Origin::Ethereum,
            false,
        );

        let mut waitlist = Waitlist::default();
        waitlist.wait(dispatch.clone(), 1000);

        let mut restorer = Restorer::new(999);
        restorer.waitlist(program_id, &waitlist);
        assert_eq!(
            restorer.restore(),
            BTreeMap::from([(
                1000,
                BTreeSet::from([ScheduledTask::WakeMessage(program_id, dispatch.id)])
            )])
        );

        let mut restorer = Restorer::new(1000);
        restorer.waitlist(program_id, &waitlist);
        assert_eq!(restorer.restore(), BTreeMap::new());
    }

    #[test]
    fn restorer_mailbox() {
        let storage = MemStorage::default();

        let program_id = ActorId::from(1);
        let user_id = ActorId::from(2);
        let message_id = MessageId::from(3);
        let message = MailboxMessage::new(
            PayloadLookup::Direct(Payload::repeat(0xfe)),
            0xffffff,
            Origin::Ethereum,
        );

        let mut mailbox = Mailbox::default();
        mailbox.add_and_store_user_mailbox(&storage, user_id, message_id, message, 1000);
        let user_mailbox = mailbox
            .as_ref()
            .iter()
            .next()
            .map(|(&mailbox_user_id, &user_mailbox)| {
                assert_eq!(user_id, mailbox_user_id);
                storage.user_mailbox(user_mailbox).unwrap()
            })
            .unwrap();

        let mut restorer = Restorer::new(999);
        restorer.user_mailbox(program_id, user_id, &user_mailbox);
        assert_eq!(
            restorer.restore(),
            BTreeMap::from([(
                1000,
                BTreeSet::from([ScheduledTask::RemoveFromMailbox(
                    (program_id, user_id),
                    message_id
                )])
            )]),
        );

        let mut restorer = Restorer::new(1000);
        restorer.user_mailbox(program_id, user_id, &user_mailbox);
        assert_eq!(restorer.restore(), BTreeMap::new());
    }

    #[test]
    fn restorer_stash() {
        let program_id = ActorId::from(1);
        let program_dispatch = Dispatch::reply(
            MessageId::from(456),
            ActorId::from(789),
            PayloadLookup::Direct(Payload::repeat(0xfe)),
            0xffffff,
            SuccessReplyReason::Auto,
            Origin::Ethereum,
            false,
        );

        let user_id = ActorId::from(2);
        let user_dispatch = Dispatch::reply(
            MessageId::from(789),
            ActorId::from(999),
            PayloadLookup::Direct(Payload::repeat(0xaa)),
            0xbbbbbb,
            SuccessReplyReason::Auto,
            Origin::Ethereum,
            false,
        );

        let mut stash = DispatchStash::default();
        stash.add_to_program(program_dispatch.clone(), 1000);
        stash.add_to_user(user_dispatch.clone(), 1000, user_id);

        let mut restorer = Restorer::new(999);
        restorer.stash(program_id, &stash);
        assert_eq!(
            restorer.restore(),
            BTreeMap::from([(
                1000,
                BTreeSet::from([
                    ScheduledTask::SendDispatch((program_id, program_dispatch.id)),
                    ScheduledTask::SendUserMessage {
                        message_id: user_dispatch.id,
                        to_mailbox: program_id
                    }
                ])
            )]),
        );

        let mut restorer = Restorer::new(1000);
        restorer.stash(program_id, &stash);
        assert_eq!(restorer.restore(), BTreeMap::new());
    }
}
