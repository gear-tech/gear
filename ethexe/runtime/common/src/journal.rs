// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use crate::{
    TransitionController,
    state::{
        ActiveProgram, Dispatch, Expiring, MAILBOX_VALIDITY, MailboxMessage, ModifiableStorage,
        Program, ProgramState, Storage,
    },
};
use alloc::{
    collections::{BTreeMap, BTreeSet},
    vec::Vec,
};
use core::{mem, num::NonZero, panic};
use core_processor::common::{DispatchOutcome, JournalHandler, JournalNote};
use ethexe_common::{
    ScheduledTask,
    gear::{INJECTED_MESSAGE_PANIC_GAS_CHARGE_THRESHOLD, Message, MessageType},
};
use gear_core::{
    env::MessageWaitedType,
    gas::GasAllowanceCounter,
    memory::PageBuf,
    message::{Dispatch as CoreDispatch, StoredDispatch},
    pages::{GearPage, WasmPage, num_traits::Zero as _, numerated::tree::IntervalsTree},
    reservation::GasReserver,
};
use gear_core_errors::SignalCode;
use gprimitives::{ActorId, CodeId, H256, MessageId, ReservationId};
use gsys::GasMultiplier;

/// Maximum duration for gr_wait_up_to in blocks,
/// when not enough gas was provided for the requested duration.
pub const WAIT_UP_TO_SAFE_DURATION: u32 = 64;

// Handles unprocessed journal notes during chunk processing.
pub struct NativeJournalHandler<'a, S: Storage + ?Sized> {
    pub program_id: ActorId,
    pub message_type: MessageType,
    pub call_reply: bool,
    pub controller: TransitionController<'a, S>,
    pub gas_allowance_counter: &'a GasAllowanceCounter,
    pub chunk_gas_limit: u64,
    pub out_of_gas: &'a mut bool,
    pub outgoing_messages_limiter: &'a mut u32,
    pub outgoing_messages_bytes_limiter: &'a mut u32,
    pub call_reply_limiter: &'a mut u32,
}

impl<S: Storage + ?Sized> NativeJournalHandler<'_, S> {
    fn send_dispatch_to_program(
        &mut self,
        _message_id: MessageId,
        destination: ActorId,
        dispatch: Dispatch,
        delay: u32,
    ) {
        if !dispatch.value.is_zero() {
            let source = dispatch.source;
            // Decrease sender's balance and value_to_receive
            self.controller
                .update_state(source, |state, _, transitions| {
                    state.balance = state.balance.checked_sub(dispatch.value).expect(
                        "Insufficient balance: underflow in state.balance -= dispatch.value()",
                    );

                    transitions.modify_transition(source, |transition| {
                        transition.value_to_receive = transition
                            .value_to_receive
                            .checked_sub(i128::try_from(dispatch.value).expect("value fits into i128"))
                            .expect("Insufficient balance: underflow in transition.value_to_receive -= dispatch.value()");
                    });

                });
        }

        self.controller
            .update_state(destination, |state, storage: &S, transitions| {
                if let Ok(non_zero_delay) = delay.try_into() {
                    let expiry = transitions.schedule_task(
                        non_zero_delay,
                        ScheduledTask::SendDispatch((destination, dispatch.id)),
                    );

                    storage.modify(&mut state.stash_hash, |stash| {
                        stash.add_to_program(dispatch, expiry);
                    });
                } else {
                    let queue = state.queue_from_msg_type(dispatch.message_type);
                    queue.modify_queue(storage, |queue| queue.queue(dispatch));
                }
            })
    }

    fn send_dispatch_to_user(
        &mut self,
        _message_id: MessageId,
        dispatch: StoredDispatch,
        delay: u32,
    ) {
        // TODO: #5227 delay must be taken into account
        *self.outgoing_messages_limiter = self.outgoing_messages_limiter.saturating_sub(1);
        *self.outgoing_messages_bytes_limiter =
            self.outgoing_messages_bytes_limiter.saturating_sub(
                u32::try_from(dispatch.payload_bytes().len())
                    .expect("payload size is too big for u32 in outgoing messages bytes limiter"),
            );
        if dispatch.is_reply() && self.call_reply {
            *self.call_reply_limiter = self.call_reply_limiter.saturating_sub(1);
        }

        if dispatch.is_reply() {
            self.controller
                .update_state(dispatch.source(), |state, _, transitions| {
                    if dispatch.value() != 0 {
                        state.balance = state.balance.checked_sub(dispatch.value()).expect(
                            "Insufficient balance: underflow in state.balance -= dispatch.value()",
                        );
                    }

                    transitions.modify_transition(dispatch.source(), |transition| {
                        let stored = dispatch.into_parts().1;

                        transition
                            .messages
                            .push(Message::from_stored(stored, self.call_reply))
                    });
                });

            return;
        }

        let message_type = self.message_type;

        self.controller
            .update_state(dispatch.source(), |state, storage, transitions| {
                let value = dispatch.value();

                if !value.is_zero() {
                    state.balance = state.balance.checked_sub(value).expect(
                        "Insufficient balance: underflow in state.balance -= dispatch.value()",
                    );

                    transitions.modify_transition(dispatch.source(), |transition| {
                        transition.value_to_receive = transition
                            .value_to_receive
                            .checked_sub(i128::try_from(value).expect("value fits into i128"))
                            .expect("Insufficient balance: underflow in transition.value_to_receive -= dispatch.value()");
                    });
                }

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
                        Dispatch::from_core_stored(storage, dispatch, message_type, false);

                    storage.modify(&mut state.stash_hash, |stash| {
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

                    let message = MailboxMessage::new(payload, dispatch.value(), message_type);

                    storage.modify(&mut state.mailbox_hash, |mailbox| {
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

impl<S: Storage + ?Sized> JournalHandler for NativeJournalHandler<'_, S> {
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
                state.balance = state.balance.checked_add(balance).expect(
                    "Overflow in state.balance += balance during exit dispatch value transfer",
                );
            })
        }
    }

    fn message_consumed(&mut self, message_id: MessageId) {
        let program_id = self.program_id;

        self.controller
            .update_state(program_id, |state, storage, _| {
                let queue = state.queue_from_msg_type(self.message_type);

                queue.modify_queue(storage, |queue| {
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
        // Reservations are deprecated and gas_limited message dispatches are not supported anymore.
        if reservation.is_some() || dispatch.gas_limit().map(|v| v != 0).unwrap_or(false) {
            unreachable!("deprecated: {dispatch:?}");
        }

        let destination = dispatch.destination();
        let dispatch = dispatch.into_stored();

        if self.controller.transitions.is_program(&destination) {
            let dispatch = Dispatch::from_core_stored(
                self.controller.storage,
                dispatch,
                self.message_type,
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
        waited_type: MessageWaitedType,
    ) {
        let Some(mut duration) = duration else {
            unreachable!("Wait dispatch without specified duration is forbidden in ethexe runtime");
        };

        match waited_type {
            MessageWaitedType::Wait => unreachable!("gr_wait is forbidden in ethexe runtime"),
            MessageWaitedType::WaitUpTo => {
                // If not gas was not enough for duration, we use safe duration as max
                duration = duration.min(WAIT_UP_TO_SAFE_DURATION);
            }
            MessageWaitedType::WaitFor | MessageWaitedType::WaitUpToFull => {}
        }

        let in_blocks =
            NonZero::<u32>::try_from(duration).expect("must be checked on backend side");

        let program_id = self.program_id;
        let message_type = self.message_type;
        let call_reply = self.call_reply;

        self.controller
            .update_state(program_id, |state, storage, transitions| {
                let expiry = transitions.schedule_task(
                    in_blocks,
                    ScheduledTask::WakeMessage(dispatch.destination(), dispatch.id()),
                );

                let dispatch =
                    Dispatch::from_core_stored(storage, dispatch, message_type, call_reply);

                let queue = state.queue_from_msg_type(message_type);

                queue.modify_queue(storage, |queue| {
                    let head = queue
                        .dequeue()
                        .expect("an attempt to wait message from empty queue");

                    assert_eq!(
                        head.id, dispatch.id,
                        "queue head doesn't match processed message"
                    );
                });

                storage.modify(&mut state.waitlist_hash, |waitlist| {
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
            unreachable!("delayed wake is forbidden in ethexe runtime");
        }

        log::trace!("Dispatch {message_id} tries to wake {awakening_id}");

        self.controller
            .update_state(program_id, |state, storage, transitions| {
                let Some(Expiring {
                    value: dispatch,
                    expiry,
                }) = storage.modify(&mut state.waitlist_hash, |waitlist| {
                    waitlist.wake(&awakening_id)
                })
                else {
                    return;
                };

                let queue = state.queue_from_msg_type(dispatch.message_type);
                queue.modify_queue(storage, |queue| queue.queue(dispatch));

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
        if value.is_zero() {
            // Nothing to do
            return;
        }

        let src_is_prog = self.controller.transitions.is_program(&from);
        let dst_is_prog = self.controller.transitions.is_program(&to);

        match (src_is_prog, dst_is_prog) {
            // User to Program or Program to Program value transfer
            (_, true) => {
                self.controller.update_state(to, |state, _, transitions| {
                    state.balance = state
                        .balance
                        .checked_add(value)
                        .expect("Overflow in state.balance += value during value transfer");

                    transitions.modify_transition(to, |transition| {
                        transition.value_to_receive = transition
                            .value_to_receive
                            .checked_add(i128::try_from(value).expect("value fits into i128"))
                            .expect("Overflow in transition.value_to_receive += value");
                    });
                });
            }
            (true, false) => {
                // Program to User value transfer
                unreachable!("Program to User value transfer is not supported");
            }
            (false, false) => {
                // User to User value transfer is not supported
                unreachable!("User to User value transfer is not supported");
            }
        }
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
            *self.out_of_gas = true;
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
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RuntimeQueueReport {
    pub dispatched: Vec<RuntimeDispatchReport>,
    pub gas_burned: Vec<RuntimeGasBurnReport>,
}

impl RuntimeQueueReport {
    pub fn extend(&mut self, other: Self) {
        self.dispatched.extend(other.dispatched);
        self.gas_burned.extend(other.gas_burned);
    }
}

#[derive(Clone, Debug)]
pub struct RuntimeDispatchReport {
    pub message_id: MessageId,
    pub source: ActorId,
    pub outcome: DispatchOutcome,
}

impl PartialEq for RuntimeDispatchReport {
    fn eq(&self, other: &Self) -> bool {
        self.message_id == other.message_id
            && self.source == other.source
            && dispatch_outcome_eq(&self.outcome, &other.outcome)
    }
}

impl Eq for RuntimeDispatchReport {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeGasBurnReport {
    pub message_id: MessageId,
    pub amount: u64,
    pub charged_to_executable_balance: bool,
}

fn dispatch_outcome_eq(left: &DispatchOutcome, right: &DispatchOutcome) -> bool {
    match (left, right) {
        (
            DispatchOutcome::Exit {
                program_id: left_program_id,
            },
            DispatchOutcome::Exit {
                program_id: right_program_id,
            },
        )
        | (
            DispatchOutcome::InitSuccess {
                program_id: left_program_id,
            },
            DispatchOutcome::InitSuccess {
                program_id: right_program_id,
            },
        ) => left_program_id == right_program_id,
        (
            DispatchOutcome::InitFailure {
                program_id: left_program_id,
                origin: left_origin,
                reason: left_reason,
            },
            DispatchOutcome::InitFailure {
                program_id: right_program_id,
                origin: right_origin,
                reason: right_reason,
            },
        ) => {
            left_program_id == right_program_id
                && left_origin == right_origin
                && left_reason == right_reason
        }
        (
            DispatchOutcome::MessageTrap {
                program_id: left_program_id,
                trap: left_trap,
            },
            DispatchOutcome::MessageTrap {
                program_id: right_program_id,
                trap: right_trap,
            },
        ) => left_program_id == right_program_id && left_trap == right_trap,
        (DispatchOutcome::Success, DispatchOutcome::Success)
        | (DispatchOutcome::NoExecution, DispatchOutcome::NoExecution) => true,
        _ => false,
    }
}

pub struct RuntimeJournalHandler<'s, S>
where
    S: Storage,
{
    pub storage: &'s S,
    pub program_state: &'s mut ProgramState,
    pub gas_allowance_counter: &'s mut GasAllowanceCounter,
    pub gas_multiplier: &'s GasMultiplier,
    pub message_type: MessageType,
    pub is_first_execution: bool,
    pub stop_processing: bool,
    pub call_reply: bool,
    pub limiter: &'s mut Limiter,
}

impl<S> RuntimeJournalHandler<'_, S>
where
    S: Storage,
{
    // Returns unhandled journal notes, new program state hash, and runtime queue report
    pub fn handle_journal_with_report<I>(
        &mut self,
        journal: I,
    ) -> (Vec<JournalNote>, Option<H256>, RuntimeQueueReport)
    where
        I: IntoIterator<Item = JournalNote>,
        I::IntoIter: ExactSizeIterator,
    {
        let journal = journal.into_iter();
        let mut page_updates = BTreeMap::new();
        let mut allocations_update = BTreeMap::new();
        let notes_count = journal.len();
        let mut skipped_notes = 0;
        let mut report = RuntimeQueueReport::default();

        // The set of panic injected messages for which we do not charge executable balance.
        // Dispatches for these messages will not be include into filtered journal notes.
        let mut messages_to_skip = BTreeSet::new();

        let filtered: Vec<_> = journal
            .filter_map(|note| {
                match note {
                    JournalNote::MessageDispatched {
                        message_id,
                        source,
                        outcome,
                    } => {
                        report.dispatched.push(RuntimeDispatchReport {
                            message_id,
                            source,
                            outcome: outcome.clone(),
                        });
                        self.message_dispatched(message_id, source, outcome);
                    }
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
                        message_id,
                        amount,
                        is_panic,
                    } => {
                        self.gas_allowance_counter.charge(amount);

                        // Special case for panicked `Injected` messages with gas spent less than the threshold.
                        let charged_to_executable_balance =
                            !is_panic || self.should_charge_exec_balance_on_panic(amount);

                        report.gas_burned.push(RuntimeGasBurnReport {
                            message_id,
                            amount,
                            charged_to_executable_balance,
                        });

                        if charged_to_executable_balance {
                            self.charge_exec_balance(amount);
                        } else {
                            // Message panic and we do not charge exec balance - do not include to journal.
                            messages_to_skip.insert(message_id);
                        }
                    }
                    note @ JournalNote::StopProcessing {
                        dispatch: _,
                        gas_burned,
                    } => {
                        self.gas_allowance_counter.charge(gas_burned);
                        self.stop_processing = true;
                        return Some(note);
                    }
                    // TODO: #5228 handle the listed journal notes here:
                    // * WakeMessage
                    // * SendDispatch to self
                    // * SendValue to self
                    note => {
                        match &note {
                            JournalNote::SendDispatch { message_id, .. }
                                if messages_to_skip.contains(message_id) =>
                            {
                                return None;
                            }
                            JournalNote::SendDispatch { dispatch, .. } => {
                                // TODO: #5227 delay must be taken into account
                                self.limiter.outgoing_messages =
                                    self.limiter.outgoing_messages.saturating_sub(1);
                                self.limiter.outgoing_messages_bytes =
                                    self.limiter.outgoing_messages_bytes.saturating_sub(
                                        u32::try_from(dispatch.payload_bytes().len())
                                            .expect("payload size is too big for u32"),
                                    );

                                if dispatch.is_reply() && self.call_reply {
                                    self.limiter.call_replies =
                                        self.limiter.call_replies.saturating_sub(1);
                                }
                            }
                            _ => {}
                        }

                        skipped_notes += 1;
                        return Some(note);
                    }
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
        let maybe_state_hash = (notes_count != skipped_notes)
            .then(|| self.storage.write_program_state(*self.program_state));

        (filtered, maybe_state_hash, report)
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

        self.storage.modify(pages_hash, |pages| {
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

        let removed_pages = self.storage.modify(allocations_hash, |allocations| {
            allocations.update(new_allocations)
        });

        if !removed_pages.is_empty() {
            self.storage.modify(pages_hash, |pages| {
                pages.remove_and_store_regions(self.storage, &removed_pages);
            })
        }
    }

    fn charge_exec_balance(&mut self, gas_burned: u64) {
        let spent_value = self.gas_multiplier.gas_to_value(gas_burned);
        self.program_state.executable_balance = self
            .program_state
            .executable_balance
            .checked_sub(spent_value)
            .expect(
                "Insufficient executable balance: underflow in executable_balance -= gas_burned",
            );
    }

    // Special case for panicked `Injected` messages with gas spent less than `INJECTED_MESSAGE_PANIC_GAS_CHARGE_THRESHOLD`.
    fn should_charge_exec_balance_on_panic(&self, gas_burned: u64) -> bool {
        gas_burned > INJECTED_MESSAGE_PANIC_GAS_CHARGE_THRESHOLD
            || self.message_type != MessageType::Injected
            || !self.is_first_execution
    }
}

pub(crate) struct Limiter {
    pub outgoing_messages: u32,
    pub outgoing_messages_bytes: u32,
    pub call_replies: u32,
}

#[derive(Debug)]
pub(crate) enum LimitsStatus {
    WithinLimits,
    OutgoingMessagesLimitExceeded,
    OutgoingMessagesBytesLimitExceeded,
    CallRepliesLimitExceeded,
}

impl Limiter {
    pub fn status(&self) -> LimitsStatus {
        if self.outgoing_messages == 0 {
            LimitsStatus::OutgoingMessagesLimitExceeded
        } else if self.outgoing_messages_bytes == 0 {
            LimitsStatus::OutgoingMessagesBytesLimitExceeded
        } else if self.call_replies == 0 {
            LimitsStatus::CallRepliesLimitExceeded
        } else {
            LimitsStatus::WithinLimits
        }
    }
}

#[cfg(test)]
mod tests {
    use gear_core::message::{DispatchKind, Message as CoreMessage, StoredMessage};

    use super::*;

    use crate::state::MemStorage;

    fn init_setup(
        exec_balance: u128,
        message_type: MessageType,
        is_first_execution: bool,
    ) -> RuntimeJournalHandler<'static, MemStorage> {
        const INITIAL_GAS_ALLOWANCE: u64 = 1_000_000_000_000;

        let storage = Box::leak(Box::new(MemStorage::default()));
        let program_state = {
            let mut ps = ProgramState::zero();
            ps.executable_balance = exec_balance;
            Box::leak(Box::new(ps))
        };
        let gas_allowance_counter =
            Box::leak(Box::new(GasAllowanceCounter::new(INITIAL_GAS_ALLOWANCE)));
        let gas_multiplier = Box::leak(Box::new(GasMultiplier::from_value_per_gas(100)));
        let limiter = Box::leak(Box::new(Limiter {
            outgoing_messages: 32,
            outgoing_messages_bytes: 4 * 1024,
            call_replies: 16,
        }));

        RuntimeJournalHandler {
            storage,
            program_state,
            gas_allowance_counter,
            gas_multiplier,
            message_type,
            is_first_execution,
            stop_processing: false,
            call_reply: false,
            limiter,
        }
    }

    #[test]
    fn charge_exec_balance() {
        const INITIAL_EXEC_BALANCE: u128 = 500_000_000_000;

        // Special case: Injected message first execution with panic and gas burned less than threshold
        let mut handler = init_setup(INITIAL_EXEC_BALANCE, MessageType::Injected, true);
        handler.handle_journal_with_report(vec![JournalNote::GasBurned {
            message_id: MessageId::new([0u8; 32]),
            amount: INJECTED_MESSAGE_PANIC_GAS_CHARGE_THRESHOLD,
            is_panic: true,
        }]);
        assert_eq!(
            handler.program_state.executable_balance,
            INITIAL_EXEC_BALANCE
        );

        // Normal cases:
        for message_type in [MessageType::Injected, MessageType::Canonical] {
            for is_panic in [false, true] {
                for amount in [
                    INJECTED_MESSAGE_PANIC_GAS_CHARGE_THRESHOLD,
                    INJECTED_MESSAGE_PANIC_GAS_CHARGE_THRESHOLD + 1,
                ] {
                    for is_first_execution in [true, false] {
                        // Skip special case already tested above
                        if message_type == MessageType::Injected
                            && is_panic
                            && is_first_execution
                            && amount == INJECTED_MESSAGE_PANIC_GAS_CHARGE_THRESHOLD
                        {
                            continue;
                        }

                        let mut handler =
                            init_setup(INITIAL_EXEC_BALANCE, message_type, is_first_execution);
                        handler.handle_journal_with_report(vec![JournalNote::GasBurned {
                            message_id: MessageId::new([0u8; 32]),
                            amount,
                            is_panic,
                        }]);
                        let expected_exec_balance =
                            INITIAL_EXEC_BALANCE - handler.gas_multiplier.gas_to_value(amount);
                        assert_eq!(
                            handler.program_state.executable_balance,
                            expected_exec_balance
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn runtime_journal_handler_reports_dispatches_and_gas() {
        const INITIAL_EXEC_BALANCE: u128 = 500_000_000_000;

        let mut handler = init_setup(INITIAL_EXEC_BALANCE, MessageType::Canonical, true);
        let message_id = MessageId::from(42);
        let source = ActorId::from(7);

        let (filtered, _hash, report) = handler.handle_journal_with_report(vec![
            JournalNote::MessageDispatched {
                message_id,
                source,
                outcome: DispatchOutcome::Success,
            },
            JournalNote::GasBurned {
                message_id,
                amount: 123,
                is_panic: false,
            },
        ]);

        assert!(filtered.is_empty());
        assert_eq!(report.dispatched.len(), 1);
        assert_eq!(report.dispatched[0].message_id, message_id);
        assert!(matches!(
            report.dispatched[0].outcome,
            DispatchOutcome::Success
        ));
        assert_eq!(report.gas_burned.len(), 1);
        assert_eq!(report.gas_burned[0].message_id, message_id);
        assert_eq!(report.gas_burned[0].amount, 123);
        assert!(report.gas_burned[0].charged_to_executable_balance);
    }

    #[test]
    fn runtime_journal_handler_reports_injected_panic_charge_exception() {
        const INITIAL_EXEC_BALANCE: u128 = 500_000_000_000;

        let message_id = MessageId::from(42);
        let mut handler = init_setup(INITIAL_EXEC_BALANCE, MessageType::Injected, true);

        let (_filtered, _hash, report) = handler.handle_journal_with_report(vec![
            JournalNote::GasBurned {
                message_id,
                amount: INJECTED_MESSAGE_PANIC_GAS_CHARGE_THRESHOLD,
                is_panic: true,
            },
            JournalNote::GasBurned {
                message_id,
                amount: INJECTED_MESSAGE_PANIC_GAS_CHARGE_THRESHOLD + 1,
                is_panic: true,
            },
        ]);

        assert_eq!(report.gas_burned.len(), 2);
        assert!(!report.gas_burned[0].charged_to_executable_balance);
        assert!(report.gas_burned[1].charged_to_executable_balance);
    }

    #[test]
    fn notes_update_state_hash() {
        let mut handler = init_setup(500_000_000_000, MessageType::Canonical, true);

        // Note unhandled (not processed in RuntimeJournalHandler)
        let (unhandled, state_hash, _) =
            handler.handle_journal_with_report(vec![JournalNote::SendDispatch {
                message_id: MessageId::new([1u8; 32]),
                dispatch: CoreDispatch::new(
                    DispatchKind::Handle,
                    CoreMessage::new(
                        MessageId::new([2u8; 32]),
                        ActorId::new([1u8; 32]),
                        ActorId::new([2u8; 32]),
                        Default::default(),
                        None,
                        0,
                        None,
                    ),
                ),
                delay: 0,
                reservation: None,
            }]);

        assert_eq!(unhandled.len(), 1);
        assert!(state_hash.is_none());

        // Note will be processed in here (in RuntimeJournalHandler) and also forwarded to `NativeJournalHandler`
        // and produce state hash update.
        let (unhandled, state_hash, _) =
            handler.handle_journal_with_report(vec![JournalNote::StopProcessing {
                dispatch: StoredDispatch::new(
                    DispatchKind::Handle,
                    StoredMessage::new(
                        MessageId::new([2u8; 32]),
                        ActorId::new([3u8; 32]),
                        ActorId::new([4u8; 32]),
                        Default::default(),
                        0,
                        None,
                    ),
                    None,
                ),
                gas_burned: 1000,
            }]);

        assert_eq!(unhandled.len(), 1);
        assert!(state_hash.is_some());

        // Note only processed in here (in RuntimeJournalHandler) and produce state hash update.
        let (unhandled, state_hash, _) =
            handler.handle_journal_with_report(vec![JournalNote::UpdatePage {
                program_id: ActorId::new([1u8; 32]),
                page_number: 16.into(),
                data: PageBuf::new_zeroed(),
            }]);
        assert!(unhandled.is_empty());
        assert!(state_hash.is_some());
    }
}
