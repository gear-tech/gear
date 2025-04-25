use crate::{
    state::{
        ActiveProgram, Dispatch, Expiring, MailboxMessage, Program, Storage, MAILBOX_VALIDITY,
    },
    TransitionController,
};
use alloc::{collections::BTreeMap, vec::Vec};
use anyhow::bail;
use core::{mem, num::NonZero};
use core_processor::common::{DispatchOutcome, JournalHandler};
use ethexe_common::{db::ScheduledTask, gear::Origin};
use gear_core::{
    ids::ProgramId,
    memory::PageBuf,
    message::{Dispatch as CoreDispatch, MessageWaitedType, StoredDispatch},
    pages::{numerated::tree::IntervalsTree, GearPage, WasmPage},
    reservation::GasReserver,
};
use gear_core_errors::SignalCode;
use gprimitives::{ActorId, CodeId, MessageId, ReservationId};

pub struct Handler<'a, S: Storage> {
    pub program_id: ProgramId,
    pub dispatch_origin: Origin,
    pub controller: TransitionController<'a, S>,
}

impl<S: Storage> Handler<'_, S> {
    fn send_dispatch_to_program(
        &mut self,
        _message_id: MessageId,
        destination: ActorId,
        dispatch: Dispatch,
        delay: u32,
    ) {
        self.controller
            .update_state(destination, |state, storage, transitions| {
                if let Ok(non_zero_delay) = delay.try_into() {
                    let expiry = transitions.schedule_task(
                        non_zero_delay,
                        ScheduledTask::SendDispatch((destination, dispatch.id)),
                    );

                    state.stash_hash.modify_stash(storage, |stash| {
                        stash.add_to_program(dispatch.id, dispatch, expiry);
                    })
                } else {
                    state
                        .queue_hash
                        .modify_queue(storage, |queue| queue.queue(dispatch));
                }
            })
    }

    fn send_dispatch_to_user(
        &mut self,
        _message_id: MessageId,
        dispatch: StoredDispatch,
        delay: u32,
    ) {
        if dispatch.is_reply() {
            self.controller
                .transitions
                .modify_transition(dispatch.source(), |transition| {
                    transition.messages.push(dispatch.into_parts().1.into())
                });

            return;
        }

        let dispatch_origin = self.dispatch_origin;

        self.controller
            .update_state(dispatch.source(), |state, storage, transitions| {
                if let Ok(non_zero_delay) = delay.try_into() {
                    let expiry = transitions.schedule_task(
                        non_zero_delay,
                        ScheduledTask::SendUserMessage {
                            message_id: dispatch.id(),
                            to_mailbox: dispatch.source(),
                        },
                    );

                    let user_id = dispatch.destination();
                    let dispatch = Dispatch::from_core_stored(storage, dispatch, dispatch_origin);

                    state.stash_hash.modify_stash(storage, |stash| {
                        stash.add_to_user(dispatch.id, dispatch, expiry, user_id);
                    });
                } else {
                    let expiry = transitions.schedule_task(
                        MAILBOX_VALIDITY.try_into().expect("infallible"),
                        ScheduledTask::RemoveFromMailbox(
                            (dispatch.source(), dispatch.destination()),
                            dispatch.id(),
                        ),
                    );

                    // TODO (breathx): remove allocation
                    let payload = storage
                        .write_payload_raw(dispatch.payload_bytes().to_vec())
                        .expect("failed to write payload");

                    let message = MailboxMessage::new(payload, dispatch.value(), dispatch_origin);

                    state.mailbox_hash.modify_mailbox(storage, |mailbox| {
                        mailbox.add_and_store_user_mailbox(
                            storage,
                            dispatch.destination(),
                            dispatch.id(),
                            message,
                            expiry,
                        )
                    });

                    transitions.modify_transition(dispatch.source(), |transition| {
                        transition.messages.push(dispatch.into_parts().1.into())
                    });
                }
            });
    }
}

impl<S: Storage> JournalHandler for Handler<'_, S> {
    fn message_dispatched(
        &mut self,
        message_id: MessageId,
        _source: ProgramId,
        outcome: DispatchOutcome,
    ) {
        match outcome {
            DispatchOutcome::Exit { program_id, .. } => {
                log::trace!("Dispatch outcome exit: {message_id} for program {program_id}")
            }

            DispatchOutcome::InitSuccess { program_id } => {
                log::trace!("Dispatch {message_id} successfully initialized program {program_id}");

                self.controller
                    .update_state(program_id, |state, _, _| {
                        match &mut state.program {
                            Program::Active(ActiveProgram { initialized, .. }) if *initialized => {
                                bail!("an attempt to initialize already initialized program")
                            }
                            &mut Program::Active(ActiveProgram {
                                ref mut initialized,
                                ..
                            }) => *initialized = true,
                            _ => bail!("an attempt to dispatch init message for inactive program"),
                        };

                        Ok(())
                    })
                    .expect("failed to update state");
            }

            DispatchOutcome::InitFailure {
                program_id,
                origin,
                reason,
            } => {
                log::trace!("Dispatch {message_id} failed init of program {program_id}: {reason}");

                self.controller.update_state(program_id, |state, _, _| {
                    state.program = Program::Terminated(origin)
                });
            }

            DispatchOutcome::MessageTrap { program_id, trap } => {
                log::trace!("Dispatch {message_id} trapped");
                log::debug!("🪤 Program {program_id} terminated with a trap: {trap}");
            }

            DispatchOutcome::Success => log::trace!("Dispatch {message_id} succeed"),

            DispatchOutcome::NoExecution => log::trace!("Dispatch {message_id} wasn't executed"),
        }
    }

    fn gas_burned(&mut self, _message_id: MessageId, _amount: u64) {
        // TODO
        // unreachable!("Must not be called here")
    }

    fn exit_dispatch(&mut self, id_exited: ProgramId, value_destination: ProgramId) {
        // TODO (breathx): handle rest of value cases; exec balance into value_to_receive.
        let balance = self
            .controller
            .update_state(id_exited, |state, _, transitions| {
                state.program = Program::Exited(value_destination);

                transitions.modify_transition(id_exited, |transition| {
                    transition.inheritor = value_destination
                });

                mem::replace(&mut state.balance, 0)
            });

        if self.controller.transitions.is_program(&value_destination) {
            self.controller
                .update_state(value_destination, |state, _, _| {
                    state.balance += balance;
                })
        }
    }

    fn message_consumed(&mut self, message_id: MessageId) {
        let program_id = self.program_id;

        self.controller
            .update_state(program_id, |state, storage, _| {
                state.queue_hash.modify_queue(storage, |queue| {
                    let head = queue
                        .dequeue()
                        .expect("an attempt to consume message from empty queue");

                    assert_eq!(
                        head.id, message_id,
                        "queue head doesn't match processed message"
                    );
                });
            })
    }

    fn send_dispatch(
        &mut self,
        message_id: MessageId,
        dispatch: CoreDispatch,
        delay: u32,
        reservation: Option<ReservationId>,
    ) {
        if reservation.is_some() || dispatch.gas_limit().map(|v| v != 0).unwrap_or(false) {
            unreachable!("deprecated: {dispatch:?}");
        }

        let destination = dispatch.destination();
        let dispatch = dispatch.into_stored();

        if self.controller.transitions.is_program(&destination) {
            let dispatch =
                Dispatch::from_core_stored(self.controller.storage, dispatch, self.dispatch_origin);

            self.send_dispatch_to_program(message_id, destination, dispatch, delay);
        } else {
            self.send_dispatch_to_user(message_id, dispatch, delay);
        }
    }

    fn wait_dispatch(
        &mut self,
        dispatch: StoredDispatch,
        duration: Option<u32>,
        _waited_type: MessageWaitedType,
    ) {
        let Some(duration) = duration else {
            todo!("Wait dispatch without specified duration");
        };

        let in_blocks =
            NonZero::<u32>::try_from(duration).expect("must be checked on backend side");

        let program_id = self.program_id;
        let dispatch_origin = self.dispatch_origin;

        self.controller
            .update_state(program_id, |state, storage, transitions| {
                let expiry = transitions.schedule_task(
                    in_blocks,
                    ScheduledTask::WakeMessage(dispatch.destination(), dispatch.id()),
                );

                let dispatch = Dispatch::from_core_stored(storage, dispatch, dispatch_origin);

                state.queue_hash.modify_queue(storage, |queue| {
                    let head = queue
                        .dequeue()
                        .expect("an attempt to wait message from empty queue");

                    assert_eq!(
                        head.id, dispatch.id,
                        "queue head doesn't match processed message"
                    );
                });

                state.waitlist_hash.modify_waitlist(storage, |waitlist| {
                    waitlist.wait(dispatch.id, dispatch, expiry);
                });
            });
    }

    // TODO (breathx): deprecate delayed wakes?
    fn wake_message(
        &mut self,
        message_id: MessageId,
        program_id: ProgramId,
        awakening_id: MessageId,
        delay: u32,
    ) {
        if delay != 0 {
            todo!("Delayed wake message");
        }

        log::trace!("Dispatch {message_id} tries to wake {awakening_id}");

        self.controller
            .update_state(program_id, |state, storage, transitions| {
                let Some(Expiring {
                    value: dispatch,
                    expiry,
                }) = state
                    .waitlist_hash
                    .modify_waitlist(storage, |waitlist| waitlist.wake(&awakening_id))
                else {
                    return;
                };

                state
                    .queue_hash
                    .modify_queue(storage, |queue| queue.queue(dispatch));

                transitions
                    .remove_task(
                        expiry,
                        &ScheduledTask::WakeMessage(program_id, awakening_id),
                    )
                    .expect("failed to remove scheduled task");
            });
    }

    fn update_pages_data(
        &mut self,
        program_id: ProgramId,
        pages_data: BTreeMap<GearPage, PageBuf>,
    ) {
        if pages_data.is_empty() {
            return;
        }

        self.controller
            .update_state(program_id, |state, storage, _| {
                let Program::Active(ActiveProgram {
                    ref mut pages_hash, ..
                }) = state.program
                else {
                    bail!("an attempt to update pages data of inactive program");
                };

                pages_hash.modify_pages(storage, |pages| {
                    pages.update_and_store_regions(storage, storage.write_pages_data(pages_data));
                });

                Ok(())
            })
            .expect("failed to update state");
    }

    fn update_allocations(
        &mut self,
        program_id: ProgramId,
        new_allocations: IntervalsTree<WasmPage>,
    ) {
        self.controller
            .update_state(program_id, |state, storage, _| {
                let Program::Active(ActiveProgram {
                    allocations_hash,
                    pages_hash,
                    ..
                }) = &mut state.program
                else {
                    bail!("an attempt to update allocations of inactive program");
                };

                allocations_hash.modify_allocations(storage, |allocations| {
                    let removed_pages = allocations.update(new_allocations);

                    if !removed_pages.is_empty() {
                        pages_hash.modify_pages(storage, |pages| {
                            pages.remove_and_store_regions(storage, &removed_pages);
                        })
                    }
                });

                Ok(())
            })
            .expect("failed to update state");
    }

    fn send_value(&mut self, from: ProgramId, to: Option<ProgramId>, value: u128) {
        // TODO (breathx): implement rest of cases.
        if let Some(to) = to {
            if self.controller.transitions.state_of(&from).is_some() {
                return;
            }

            self.controller.update_state(to, |state, _, transitions| {
                state.balance += value;

                transitions
                    .modify_transition(to, |transition| transition.value_to_receive += value);
            });
        }
    }

    fn store_new_programs(
        &mut self,
        _program_id: ProgramId,
        _code_id: CodeId,
        _candidates: Vec<(MessageId, ProgramId)>,
    ) {
        todo!()
    }

    fn stop_processing(&mut self, _dispatch: StoredDispatch, _gas_burned: u64) {
        todo!()
    }

    fn reserve_gas(&mut self, _: MessageId, _: ReservationId, _: ProgramId, _: u64, _: u32) {
        unreachable!("deprecated");
    }

    fn unreserve_gas(&mut self, _: ReservationId, _: ProgramId, _: u32) {
        unreachable!("deprecated");
    }

    fn update_gas_reservation(&mut self, _: ProgramId, _: GasReserver) {
        unreachable!("deprecated");
    }

    fn system_reserve_gas(&mut self, _: MessageId, _: u64) {
        unreachable!("deprecated");
    }

    fn system_unreserve_gas(&mut self, _: MessageId) {
        unreachable!("deprecated");
    }

    fn send_signal(&mut self, _: MessageId, _: ProgramId, _: SignalCode) {
        unreachable!("deprecated");
    }

    fn reply_deposit(&mut self, _: MessageId, _: MessageId, _: u64) {
        unreachable!("deprecated");
    }
}
