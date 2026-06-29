// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use crate::{
    TransitionController,
    state::{
        Dispatch, DispatchStash, Expiring, MailboxMessage, ModifiableStorage, PayloadLookup,
        ProgramState, QueryableStorage, Storage, UserMailbox, Waitlist,
    },
    transitions::{
        is_eth_sails_event_destination, is_event_destination, is_gear_sails_event_destination,
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
                    value:
                        MailboxMessage {
                            value,
                            message_type: origin,
                            ..
                        },
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

                let queue = state.queue_from_msg_type(origin);
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

                let queue = state.queue_from_msg_type(dispatch.message_type);
                queue.modify_queue(storage, |queue| {
                    queue.queue(dispatch);
                });
            });

        0
    }

    fn send_user_message(&mut self, stashed_message_id: MessageId, program_id: ActorId) -> u64 {
        let cfg = self.controller.transitions.cfg();
        let mailbox_validity = cfg.mailbox_validity;
        let event_destinations = cfg.event_destinations_autoreply;

        self.controller
            .update_state(program_id, |state, storage, transitions| {
                let (dispatch, user_id) = storage.modify(&mut state.stash_hash, |stash| {
                    stash.remove_to_user(&stashed_message_id)
                });

                if event_destinations && is_event_destination(user_id) {
                    let message_type = dispatch.message_type;

                    transitions.modify_transition(program_id, |transition| {
                        let message = dispatch.clone().into_message(storage, user_id);
                        if is_gear_sails_event_destination(user_id) {
                            transition.events.push(message.payload);
                        } else if is_eth_sails_event_destination(user_id) {
                            transition.eth_events.push(message.payload);
                        }
                    });

                    let reply = Dispatch::reply(
                        stashed_message_id,
                        user_id,
                        PayloadLookup::empty(),
                        0,
                        SuccessReplyReason::Auto,
                        message_type,
                        false,
                    );

                    let queue = state.queue_from_msg_type(message_type);
                    queue.modify_queue(storage, |queue| queue.queue(reply));

                    return;
                }

                let expiry = transitions.schedule_task(
                    mailbox_validity,
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

                let queue = state.queue_from_msg_type(dispatch.message_type);
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
/// Used primary for fast sync and tests.
///
/// No expiry filtering is applied: every scheduled task found in the dumped
/// states is restored. Committed states never hold a task already expired at
/// the dumped block, and the executor drains the full backlog with no lower
/// bound, so any restored task fires at the first computed block regardless of
/// its expiry.
#[derive(Default)]
pub struct Restorer {
    schedule: Schedule,
}

impl Restorer {
    /// Creates an empty restorer.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a restorer from storage.
    ///
    /// Tries to fully restore schedule
    pub fn from_storage<T: Storage>(
        storage: &T,
        program_states: &ProgramStates,
    ) -> anyhow::Result<Self> {
        let program_states: BTreeMap<H256, BTreeSet<ActorId>> =
            program_states
                .iter()
                .fold(BTreeMap::new(), |mut acc, (&program_id, state)| {
                    acc.entry(state.hash).or_default().insert(program_id);
                    acc
                });

        let mut restorer = Self::new();

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
    use ethexe_common::gear::MessageType;
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
            MessageType::Canonical,
            false,
        );

        let mut waitlist = Waitlist::default();
        waitlist.wait(dispatch.clone(), 1000);

        let mut restorer = Restorer::new();
        restorer.waitlist(program_id, &waitlist);
        assert_eq!(
            restorer.restore(),
            BTreeMap::from([(
                1000,
                BTreeSet::from([ScheduledTask::WakeMessage(program_id, dispatch.id)])
            )])
        );
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
            MessageType::Canonical,
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

        let mut restorer = Restorer::new();
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
            MessageType::Canonical,
            false,
        );

        let user_id = ActorId::from(2);
        let user_dispatch = Dispatch::reply(
            MessageId::from(789),
            ActorId::from(999),
            PayloadLookup::Direct(Payload::repeat(0xaa)),
            0xbbbbbb,
            SuccessReplyReason::Auto,
            MessageType::Canonical,
            false,
        );

        let mut stash = DispatchStash::default();
        stash.add_to_program(program_dispatch.clone(), 1000);
        stash.add_to_user(user_dispatch.clone(), 1000, user_id);

        let mut restorer = Restorer::new();
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
    }

    // Regression for #5574: the genesis `from_storage` path must restore every
    // scheduled task in the dumped states regardless of its expiry. The restorer
    // used to drop tasks with `expiry <= genesis_block_height`; during re-genesis
    // that height can sit far above a task's expiry, so still-pending tasks were
    // silently lost. Here all expiries are tiny (1..=4) — exactly the entries the
    // old cutoff would have discarded — and all four must survive restoration.
    #[test]
    fn from_storage_restores_tasks_below_genesis_height() {
        use ethexe_common::StateHashWithQueueSize;

        let storage = MemStorage::default();
        let program_id = ActorId::from(1);
        let user_id = ActorId::from(2);

        let reply = |id: u64, fill: u8| {
            Dispatch::reply(
                MessageId::from(id),
                ActorId::from(id + 1000),
                PayloadLookup::Direct(Payload::repeat(fill)),
                0xffffff,
                SuccessReplyReason::Auto,
                MessageType::Canonical,
                false,
            )
        };

        let waitlisted = reply(10, 0x11);
        let stashed_to_program = reply(11, 0x22);
        let stashed_to_user = reply(12, 0x33);
        let mailbox_message_id = MessageId::from(13);

        let mut waitlist = Waitlist::default();
        waitlist.wait(waitlisted.clone(), 1);

        let mut stash = DispatchStash::default();
        stash.add_to_program(stashed_to_program.clone(), 2);
        stash.add_to_user(stashed_to_user.clone(), 3, user_id);

        let mut mailbox = Mailbox::default();
        mailbox.add_and_store_user_mailbox(
            &storage,
            user_id,
            mailbox_message_id,
            MailboxMessage::new(
                PayloadLookup::Direct(Payload::repeat(0x44)),
                0xffffff,
                MessageType::Canonical,
            ),
            4,
        );

        let mut state = ProgramState::zero();
        state.waitlist_hash = waitlist.store(&storage).expect("waitlist changed");
        state.stash_hash = stash.store(&storage);
        state.mailbox_hash = mailbox.store(&storage).expect("mailbox changed");
        let state_hash = storage.write_program_state(state);

        let program_states = ProgramStates::from([(
            program_id,
            StateHashWithQueueSize {
                hash: state_hash,
                canonical_queue_size: 0,
                injected_queue_size: 0,
            },
        )]);

        let schedule = Restorer::from_storage(&storage, &program_states)
            .expect("restore must succeed")
            .restore();

        assert_eq!(
            schedule,
            BTreeMap::from([
                (
                    1,
                    BTreeSet::from([ScheduledTask::WakeMessage(program_id, waitlisted.id)])
                ),
                (
                    2,
                    BTreeSet::from([ScheduledTask::SendDispatch((
                        program_id,
                        stashed_to_program.id
                    ))])
                ),
                (
                    3,
                    BTreeSet::from([ScheduledTask::SendUserMessage {
                        message_id: stashed_to_user.id,
                        to_mailbox: program_id,
                    }])
                ),
                (
                    4,
                    BTreeSet::from([ScheduledTask::RemoveFromMailbox(
                        (program_id, user_id),
                        mailbox_message_id
                    )])
                ),
            ]),
        );
    }

    #[test]
    fn send_user_message_to_event_destination_skips_mailbox() {
        use crate::{
            InBlockTransitions, TransitionController, TransitionsConfig,
            transitions::{ETH_SAILS_EVENT, GEAR_SAILS_EVENT},
        };
        use ethexe_common::{ProgramStates, StateHashWithQueueSize};
        use gear_core::{
            ids::prelude::MessageIdExt,
            message::{DispatchKind, ReplyCode},
        };

        for destination in [GEAR_SAILS_EVENT, ETH_SAILS_EVENT] {
            let storage = MemStorage::default();
            let program_id = ActorId::from(7);
            let message_id = MessageId::from(10);

            let dispatch = Dispatch::new(
                &storage,
                message_id,
                program_id,
                vec![1, 2, 3],
                11,
                false,
                MessageType::Canonical,
                false,
            )
            .expect("dispatch");

            let mut stash = DispatchStash::default();
            stash.add_to_user(dispatch, 1000, destination);

            let mut state = ProgramState::zero();
            state.stash_hash = stash.store(&storage);
            let state_hash = storage.write_program_state(state);
            let states = ProgramStates::from_iter([(
                program_id,
                StateHashWithQueueSize {
                    hash: state_hash,
                    canonical_queue_size: 0,
                    injected_queue_size: 0,
                },
            )]);

            let cfg = TransitionsConfig {
                event_destinations_autoreply: true,
                ..Default::default()
            };
            let mut transitions = InBlockTransitions::new(cfg, states, Default::default());

            {
                let mut handler = Handler {
                    controller: TransitionController {
                        storage: &storage,
                        transitions: &mut transitions,
                    },
                };
                handler.send_user_message(message_id, program_id);
            }

            let transition = transitions.modifications_mut().remove(&program_id).unwrap();
            assert_eq!(transition.messages.len(), 1);
            assert_eq!(transition.messages[0].destination, destination);
            assert_eq!(transition.messages[0].value, 11);
            assert_eq!(transition.claims.len(), 1);
            assert_eq!(transition.claims[0].message_id, message_id);
            assert_eq!(transition.claims[0].destination, destination);
            assert_eq!(transition.claims[0].value, 11);

            let state_hash = transitions.state_of(&program_id).unwrap().hash;
            let state = storage.program_state(state_hash).unwrap();
            assert!(state.mailbox_hash.is_empty());

            let mut queue = state.canonical_queue.query(&storage).unwrap();
            let reply = queue.dequeue().expect("auto reply must be queued");
            assert_eq!(reply.id, MessageId::generate_reply(message_id));
            assert_eq!(reply.kind, DispatchKind::Reply);
            assert_eq!(reply.source, destination);
            assert_eq!(
                reply
                    .details
                    .unwrap()
                    .to_reply_details()
                    .unwrap()
                    .to_reply_code(),
                ReplyCode::Success(SuccessReplyReason::Auto)
            );
            assert!(queue.is_empty());
        }
    }

    #[test]
    fn send_user_message_to_regular_user_uses_mailbox() {
        use crate::{InBlockTransitions, TransitionController, TransitionsConfig};
        use ethexe_common::{ProgramStates, StateHashWithQueueSize};

        let storage = MemStorage::default();
        let program_id = ActorId::from(7);
        let user_id = ActorId::from(2);
        let message_id = MessageId::from(10);

        let dispatch = Dispatch::new(
            &storage,
            message_id,
            program_id,
            vec![1, 2, 3],
            11,
            false,
            MessageType::Canonical,
            false,
        )
        .expect("dispatch");

        let mut stash = DispatchStash::default();
        stash.add_to_user(dispatch, 1000, user_id);

        let mut state = ProgramState::zero();
        state.stash_hash = stash.store(&storage);
        let state_hash = storage.write_program_state(state);
        let states = ProgramStates::from_iter([(
            program_id,
            StateHashWithQueueSize {
                hash: state_hash,
                canonical_queue_size: 0,
                injected_queue_size: 0,
            },
        )]);

        let cfg = TransitionsConfig {
            event_destinations_autoreply: true,
            ..Default::default()
        };
        let mut transitions = InBlockTransitions::new(cfg, states, Default::default());

        {
            let mut handler = Handler {
                controller: TransitionController {
                    storage: &storage,
                    transitions: &mut transitions,
                },
            };
            handler.send_user_message(message_id, program_id);
        }

        let transition = transitions.modifications_mut().remove(&program_id).unwrap();
        assert_eq!(transition.messages.len(), 1);
        assert_eq!(transition.messages[0].destination, user_id);
        assert_eq!(transition.messages[0].value, 11);
        assert!(transition.claims.is_empty());

        let state_hash = transitions.state_of(&program_id).unwrap().hash;
        let state = storage.program_state(state_hash).unwrap();
        assert!(!state.mailbox_hash.is_empty());
        assert!(state.stash_hash.is_empty());
        assert!(state.canonical_queue.is_empty());
    }
}
