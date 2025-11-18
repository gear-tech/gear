use crate::{
    TransitionController,
    state::{
        ActiveProgram, Dispatch, Expiring, MAILBOX_VALIDITY, MailboxMessage, ModifiableStorage,
        Program, ProgramState, Storage,
    },
};
use alloc::{collections::BTreeMap, vec::Vec};
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
    message::{Dispatch as CoreDispatch, DispatchKind, StoredDispatch},
    pages::{GearPage, WasmPage, num_traits::Zero as _, numerated::tree::IntervalsTree},
    reservation::GasReserver,
    rpc::ReplyInfo,
};
use gear_core_errors::SignalCode;
use gprimitives::{ActorId, CodeId, H256, MessageId, ReservationId};
use gsys::GasMultiplier;

// Handles unprocessed journal notes during chunk processing.
pub struct NativeJournalHandler<'a, S: Storage> {
    pub program_id: ActorId,
    pub message_type: MessageType,
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

        if self.message_type == MessageType::Injected && dispatch.kind() == DispatchKind::Reply {
            let reply_info = ReplyInfo {
                payload: dispatch.payload_bytes().to_vec(),
                code: dispatch
                    .reply_code()
                    .expect("expect reply_code in dispatch with DispatchKind::Reply"),
                value: dispatch.value(),
            };

            self.controller
                .transitions
                .maybe_store_injected_reply(&message_id, reply_info);
        }

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
        _waited_type: MessageWaitedType,
    ) {
        let Some(duration) = duration else {
            todo!("Wait dispatch without specified duration");
        };

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
            todo!("Delayed wake message");
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
    pub gas_multiplier: &'s GasMultiplier,
    pub message_type: MessageType,
    pub stop_processing: bool,
}

impl<S> RuntimeJournalHandler<'_, S>
where
    S: Storage,
{
    // Returns unhandled journal notes and new program state hash
    pub fn handle_journal<I>(&mut self, journal: I) -> (Vec<JournalNote>, Option<H256>)
    where
        I: IntoIterator<Item = JournalNote>,
        I::IntoIter: ExactSizeIterator,
    {
        let journal = journal.into_iter();
        let mut page_updates = BTreeMap::new();
        let mut allocations_update = BTreeMap::new();
        let notes_count = journal.len();
        let mut skipped_notes = 0;

        let filtered: Vec<_> = journal
            .filter_map(|note| {
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
                        is_panic,
                    } => {
                        self.gas_allowance_counter.charge(amount);

                        // Special case for panicked `Injected` messages with gas spent less than the threshold.
                        if !is_panic || self.should_charge_exec_balance_on_panic(amount) {
                            self.charge_exec_balance(amount);
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
                    // TODO(romanm): handle the listed journal notes here:
                    // * WakeMessage
                    // * SendDispatch to self
                    // * SendValue to self
                    note => {
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::state::MemStorage;

    fn init_setup(
        exec_balance: u128,
        message_type: MessageType,
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

        RuntimeJournalHandler {
            storage,
            program_state,
            gas_allowance_counter,
            gas_multiplier,
            message_type,
            stop_processing: false,
        }
    }

    #[test]
    fn charge_exec_balance() {
        const INITIAL_EXEC_BALANCE: u128 = 500_000_000_000;

        // Special case: Injected message with panic and gas burned less than threshold
        let mut handler = init_setup(INITIAL_EXEC_BALANCE, MessageType::Injected);
        handler.handle_journal(vec![JournalNote::GasBurned {
            message_id: MessageId::new([0u8; 32]),
            amount: INJECTED_MESSAGE_PANIC_GAS_CHARGE_THRESHOLD,
            is_panic: true,
        }]);
        assert_eq!(
            handler.program_state.executable_balance,
            INITIAL_EXEC_BALANCE
        );

        // Normal case: Injected message with panic and gas burned more than threshold
        let mut handler = init_setup(INITIAL_EXEC_BALANCE, MessageType::Injected);
        handler.handle_journal(vec![JournalNote::GasBurned {
            message_id: MessageId::new([0u8; 32]),
            amount: INJECTED_MESSAGE_PANIC_GAS_CHARGE_THRESHOLD + 1,
            is_panic: true,
        }]);
        let expected_exec_balance = INITIAL_EXEC_BALANCE
            - handler
                .gas_multiplier
                .gas_to_value(INJECTED_MESSAGE_PANIC_GAS_CHARGE_THRESHOLD + 1);
        assert_eq!(
            handler.program_state.executable_balance,
            expected_exec_balance
        );

        // Normal case: Injected message without panic and gas burned more than threshold
        let mut handler = init_setup(INITIAL_EXEC_BALANCE, MessageType::Injected);
        handler.handle_journal(vec![JournalNote::GasBurned {
            message_id: MessageId::new([0u8; 32]),
            amount: INJECTED_MESSAGE_PANIC_GAS_CHARGE_THRESHOLD + 1,
            is_panic: false,
        }]);
        let expected_exec_balance = INITIAL_EXEC_BALANCE
            - handler
                .gas_multiplier
                .gas_to_value(INJECTED_MESSAGE_PANIC_GAS_CHARGE_THRESHOLD + 1);
        assert_eq!(
            handler.program_state.executable_balance,
            expected_exec_balance
        );

        // Normal case: Injected message without panic and gas burned less than threshold
        let mut handler = init_setup(INITIAL_EXEC_BALANCE, MessageType::Injected);
        handler.handle_journal(vec![JournalNote::GasBurned {
            message_id: MessageId::new([0u8; 32]),
            amount: INJECTED_MESSAGE_PANIC_GAS_CHARGE_THRESHOLD,
            is_panic: false,
        }]);
        let expected_exec_balance = INITIAL_EXEC_BALANCE
            - handler
                .gas_multiplier
                .gas_to_value(INJECTED_MESSAGE_PANIC_GAS_CHARGE_THRESHOLD);
        assert_eq!(
            handler.program_state.executable_balance,
            expected_exec_balance
        );

        // Normal case: Canonical message with panic
        let mut handler = init_setup(INITIAL_EXEC_BALANCE, MessageType::Canonical);
        handler.handle_journal(vec![JournalNote::GasBurned {
            message_id: MessageId::new([0u8; 32]),
            amount: 500_000,
            is_panic: true,
        }]);
        let expected_exec_balance =
            INITIAL_EXEC_BALANCE - handler.gas_multiplier.gas_to_value(500_000);
        assert_eq!(
            handler.program_state.executable_balance,
            expected_exec_balance
        );

        // Normal case: Canonical message without panic
        let mut handler = init_setup(INITIAL_EXEC_BALANCE, MessageType::Canonical);
        handler.handle_journal(vec![JournalNote::GasBurned {
            message_id: MessageId::new([0u8; 32]),
            amount: 250_000,
            is_panic: false,
        }]);
        let expected_exec_balance =
            INITIAL_EXEC_BALANCE - handler.gas_multiplier.gas_to_value(250_000);
        assert_eq!(
            handler.program_state.executable_balance,
            expected_exec_balance
        );
    }
}
