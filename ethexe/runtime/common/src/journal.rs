use crate::{
    TransitionController,
    state::{
        ActiveProgram, Dispatch, Expiring, MAILBOX_VALIDITY, MailboxMessage, Program, ProgramState,
        Storage,
    },
};
use alloc::{collections::BTreeMap, vec::Vec};
use core::{mem, num::NonZero};
use core_processor::{
    common::{DispatchOutcome, JournalHandler, JournalNote},
    configs::BlockInfo,
};
use ethexe_common::{
    ScheduledTask,
    gear::{Message, Origin},
};
use gear_core::{
    env::MessageWaitedType,
    gas::GasAllowanceCounter,
    memory::PageBuf,
    message::{Dispatch as CoreDispatch, StoredDispatch},
    pages::{GearPage, WasmPage, numerated::tree::IntervalsTree},
    reservation::GasReserver,
};
use gear_core_errors::SignalCode;
use gprimitives::{ActorId, CodeId, H256, MessageId, ReservationId};

// Handles unprocessed journal notes during chunk processing.
pub struct NativeJournalHandler<'a, S: Storage> {
    pub program_id: ActorId,
    pub dispatch_origin: Origin,
    pub call_reply: bool,
    pub controller: TransitionController<'a, S>,
    pub gas_allowance_counter: &'a GasAllowanceCounter,
    pub chunk_gas_limit: u64,
    pub out_of_gas_for_block: &'a mut bool,
}

impl<S: Storage> NativeJournalHandler<'_, S> {
    fn send_dispatch_to_program(
        &mut self,
        _message_id: MessageId,
        destination: ActorId,
        dispatch: Dispatch,
        delay: u32,
    ) {
        self.controller
            .update_state(destination, |state, storage: &S, transitions| {
                if let Ok(non_zero_delay) = delay.try_into() {
                    let expiry = transitions.schedule_task(
                        non_zero_delay,
                        ScheduledTask::SendDispatch((destination, dispatch.id)),
                    );

                    state.stash_hash.modify_stash(storage, |stash| {
                        stash.add_to_program(dispatch, expiry);
                    })
                } else {
                    state
                        .canonical_queue
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
                    let stored = dispatch.into_parts().1;

                    transition
                        .messages
                        .push(Message::from_stored(stored, self.call_reply))
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
                    let dispatch =
                        Dispatch::from_core_stored(storage, dispatch, dispatch_origin, false);

                    state.stash_hash.modify_stash(storage, |stash| {
                        stash.add_to_user(dispatch, expiry, user_id);
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
                        let stored = dispatch.into_parts().1;

                        transition
                            .messages
                            .push(Message::from_stored(stored, false))
                    });
                }
            });
    }
}

impl<S: Storage> JournalHandler for NativeJournalHandler<'_, S> {
    fn message_dispatched(
        &mut self,
        _message_id: MessageId,
        _source: ActorId,
        _outcome: DispatchOutcome,
    ) {
        unreachable!("Handled inside runtime by `RuntimeJournalHandler`")
    }

    fn gas_burned(&mut self, _message_id: MessageId, _amount: u64) {
        unreachable!("Handled inside runtime by `RuntimeJournalHandler`")
    }

    fn exit_dispatch(&mut self, id_exited: ActorId, inheritor: ActorId) {
        // TODO (breathx): handle rest of value cases; exec balance into value_to_receive.
        let balance = self
            .controller
            .update_state(id_exited, |state, _, transitions| {
                state.program = Program::Exited(inheritor);

                transitions.modify_transition(id_exited, |transition| {
                    transition.inheritor = Some(inheritor);
                });

                mem::replace(&mut state.balance, 0)
            });

        if self.controller.transitions.is_program(&inheritor) {
            self.controller.update_state(inheritor, |state, _, _| {
                state.balance += balance;
            })
        }
    }

    fn message_consumed(&mut self, message_id: MessageId) {
        let program_id = self.program_id;

        self.controller
            .update_state(program_id, |state, storage, _| {
                state.canonical_queue.modify_queue(storage, |queue| {
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
            let dispatch = Dispatch::from_core_stored(
                self.controller.storage,
                dispatch,
                self.dispatch_origin,
                false,
            );

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
        let call_reply = self.call_reply;

        self.controller
            .update_state(program_id, |state, storage, transitions| {
                let expiry = transitions.schedule_task(
                    in_blocks,
                    ScheduledTask::WakeMessage(dispatch.destination(), dispatch.id()),
                );

                let dispatch =
                    Dispatch::from_core_stored(storage, dispatch, dispatch_origin, call_reply);

                state.canonical_queue.modify_queue(storage, |queue| {
                    let head = queue
                        .dequeue()
                        .expect("an attempt to wait message from empty queue");

                    assert_eq!(
                        head.id, dispatch.id,
                        "queue head doesn't match processed message"
                    );
                });

                state.waitlist_hash.modify_waitlist(storage, |waitlist| {
                    waitlist.wait(dispatch, expiry);
                });
            });
    }

    // TODO (breathx): deprecate delayed wakes?
    fn wake_message(
        &mut self,
        message_id: MessageId,
        program_id: ActorId,
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
                    .canonical_queue
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
        _program_id: ActorId,
        _pages_data: BTreeMap<GearPage, PageBuf>,
    ) {
        unreachable!("Handled inside runtime by `RuntimeJournalHandler`")
    }

    fn update_allocations(
        &mut self,
        _program_id: ActorId,
        _new_allocations: IntervalsTree<WasmPage>,
    ) {
        unreachable!("Handled inside runtime by `RuntimeJournalHandler`")
    }

    fn send_value(&mut self, from: ActorId, to: ActorId, value: u128, _locked: bool) {
        // TODO (breathx): implement rest of cases.
        if self.controller.transitions.state_of(&from).is_some() {
            return;
        }

        self.controller.update_state(to, |state, _, transitions| {
            state.balance += value;

            transitions.modify_transition(to, |transition| transition.value_to_receive += value);
        });
    }

    fn store_new_programs(
        &mut self,
        _program_id: ActorId,
        _code_id: CodeId,
        _candidates: Vec<(MessageId, ActorId)>,
    ) {
        todo!()
    }

    fn stop_processing(&mut self, _dispatch: StoredDispatch, _gas_burned: u64) {
        // This means we are out of gas for block, not for chunk.
        if self.gas_allowance_counter.left() < self.chunk_gas_limit {
            *self.out_of_gas_for_block = true;
        }
    }

    fn reserve_gas(&mut self, _: MessageId, _: ReservationId, _: ActorId, _: u64, _: u32) {
        unreachable!("deprecated");
    }

    fn unreserve_gas(&mut self, _: ReservationId, _: ActorId, _: u32) {
        unreachable!("deprecated");
    }

    fn update_gas_reservation(&mut self, _: ActorId, _: GasReserver) {
        unreachable!("deprecated");
    }

    fn system_reserve_gas(&mut self, _: MessageId, _: u64) {
        unreachable!("deprecated");
    }

    fn system_unreserve_gas(&mut self, _: MessageId) {
        unreachable!("deprecated");
    }

    fn send_signal(&mut self, _: MessageId, _: ActorId, _: SignalCode) {
        unreachable!("deprecated");
    }

    fn reply_deposit(&mut self, _: MessageId, _: MessageId, _: u64) {
        unreachable!("deprecated");
    }
}

// Handles unprocessed journal notes during message processing in the runtime.
pub struct RuntimeJournalHandler<'s, S>
where
    S: Storage,
{
    pub storage: &'s S,
    pub program_state: &'s mut ProgramState,
    pub gas_allowance_counter: &'s mut GasAllowanceCounter,
    pub stop_processing: bool,
    pub origin: Origin,
    pub block_info: BlockInfo,
}

impl<S> RuntimeJournalHandler<'_, S>
where
    S: Storage,
{
    // Returns unhandled journal notes and new program state hash
    pub fn handle_journal(
        &mut self,
        journal: impl IntoIterator<Item = JournalNote>,
    ) -> (Vec<JournalNote>, Option<H256>) {
        let mut page_updates = BTreeMap::new();
        let mut allocations_update = BTreeMap::new();
        let mut notes_cnt = 0;

        let filtered: Vec<_> = journal
            .into_iter()
            .filter_map(|note| {
                notes_cnt += 1;

                match note {
                    JournalNote::MessageDispatched {
                        message_id,
                        source,
                        outcome,
                    } => self.message_dispatched(message_id, source, outcome),
                    JournalNote::UpdatePage {
                        program_id,
                        page_number,
                        data,
                    } => {
                        let entry = page_updates.entry(program_id).or_insert_with(BTreeMap::new);
                        entry.insert(page_number, data);
                    }
                    JournalNote::UpdateAllocations {
                        program_id,
                        allocations,
                    } => {
                        allocations_update.insert(program_id, allocations);
                    }
                    JournalNote::GasBurned {
                        message_id: _,
                        amount,
                    } => {
                        // TODO(romanm): reduce exec balance
                        self.gas_allowance_counter.charge(amount);
                    }
                    note @ JournalNote::StopProcessing {
                        dispatch: _,
                        gas_burned,
                    } => {
                        self.gas_allowance_counter.charge(gas_burned);
                        self.stop_processing = true;
                        return Some(note);
                    }
                    // TODO(romanm): handle the listed journal notes here:
                    // * WakeMessage
                    // * SendDispatch to self
                    // * SendValue to self
                    note => return Some(note),
                }

                None
            })
            .collect();

        for pages_data in page_updates.into_values() {
            self.update_pages_data(pages_data);
        }

        for allocations in allocations_update.into_values() {
            self.update_allocations(allocations);
        }

        // Some notes were processed, thus state changed
        let maybe_state_hash = (notes_cnt != filtered.len()).then(|| {
            self.update_last_modified();
            self.storage.write_program_state(*self.program_state)
        });

        (filtered, maybe_state_hash)
    }

    fn message_dispatched(
        &mut self,
        message_id: MessageId,
        _source: ActorId,
        outcome: DispatchOutcome,
    ) {
        match outcome {
            DispatchOutcome::Exit { program_id } => {
                log::trace!("Dispatch outcome exit: {message_id} for program {program_id}")
            }

            DispatchOutcome::InitSuccess { program_id } => {
                log::trace!("Dispatch {message_id} successfully initialized program {program_id}");

                match self.program_state.program {
                    Program::Active(ActiveProgram {
                        ref mut initialized,
                        ..
                    }) if *initialized => {
                        panic!("an attempt to initialize already initialized program")
                    }
                    Program::Active(ActiveProgram {
                        ref mut initialized,
                        ..
                    }) => *initialized = true,
                    _ => panic!("an attempt to dispatch init message for inactive program"),
                };
            }

            DispatchOutcome::InitFailure {
                program_id,
                origin,
                reason,
            } => {
                log::trace!("Dispatch {message_id} failed init of program {program_id}: {reason}");

                self.program_state.program = Program::Terminated(origin)
            }

            DispatchOutcome::MessageTrap { program_id, trap } => {
                log::trace!("Dispatch {message_id} trapped");
                log::debug!("🪤 Program {program_id} terminated with a trap: {trap}");
            }

            DispatchOutcome::Success => log::trace!("Dispatch {message_id} succeed"),

            DispatchOutcome::NoExecution => log::trace!("Dispatch {message_id} wasn't executed"),
        }
    }

    fn update_pages_data(&mut self, pages_data: BTreeMap<GearPage, PageBuf>) {
        if pages_data.is_empty() {
            return;
        }

        let Program::Active(ActiveProgram {
            ref mut pages_hash, ..
        }) = self.program_state.program
        else {
            panic!("an attempt to update pages data of inactive program");
        };

        pages_hash.modify_pages(self.storage, |pages| {
            pages.update_and_store_regions(self.storage, self.storage.write_pages_data(pages_data));
        });
    }

    fn update_allocations(&mut self, new_allocations: IntervalsTree<WasmPage>) {
        let Program::Active(ActiveProgram {
            allocations_hash,
            pages_hash,
            ..
        }) = &mut self.program_state.program
        else {
            panic!("an attempt to update allocations of inactive program");
        };

        let removed_pages = allocations_hash.modify_allocations(self.storage, |allocations| {
            allocations.update(new_allocations)
        });

        if !removed_pages.is_empty() {
            pages_hash.modify_pages(self.storage, |pages| {
                pages.remove_and_store_regions(self.storage, &removed_pages);
            })
        }
    }

    fn update_last_modified(&mut self) {
        if Origin::Ethereum == self.origin {
            self.program_state.last_modified_from_eth = self.block_info.height;
        }
    }
}
