// This file is part of Gear.

// Copyright (C) 2024 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

/// Implementation of the `JournalHandler` trait for the `ExtManager`.
use std::collections::BTreeMap;

use super::{Balance, ExtManager, Gas, GenuineProgram, MintMode, Program, TestActor};
use core_processor::common::{DispatchOutcome, JournalHandler};
use gear_common::{scheduler::ScheduledTask, Origin};
use gear_core::{
    code::{Code, CodeAndId, InstrumentedCodeAndId},
    ids::{CodeId, MessageId, ProgramId, ReservationId},
    memory::PageBuf,
    message::{Dispatch, MessageWaitedType, SignalMessage, StoredDispatch},
    pages::{
        numerated::{iterators::IntervalIterator, tree::IntervalsTree},
        GearPage, WasmPage,
    },
    reservation::GasReserver,
};
use gear_core_errors::SignalCode;
use gear_wasm_instrument::gas_metering::Schedule;

impl JournalHandler for ExtManager {
    fn message_dispatched(
        &mut self,
        message_id: MessageId,
        _source: ProgramId,
        outcome: DispatchOutcome,
    ) {
        match outcome {
            DispatchOutcome::MessageTrap { .. } => {
                self.failed.insert(message_id);
            }
            DispatchOutcome::NoExecution => {
                self.not_executed.insert(message_id);
            }
            DispatchOutcome::Success | DispatchOutcome::Exit { .. } => {
                self.succeed.insert(message_id);
            }
            DispatchOutcome::InitFailure { program_id, .. } => {
                self.init_failure(program_id);
                self.failed.insert(message_id);
            }
            DispatchOutcome::InitSuccess { program_id, .. } => {
                self.init_success(program_id);
                self.succeed.insert(message_id);
            }
        }
    }

    fn gas_burned(&mut self, message_id: MessageId, amount: u64) {
        self.gas_allowance = self.gas_allowance.saturating_sub(Gas(amount));
        self.gas_tree
            .spend(message_id, amount)
            .unwrap_or_else(|e| unreachable!("GasTree corrupted! {:?}", e));

        self.gas_burned
            .entry(message_id)
            .and_modify(|gas| {
                *gas += Gas(amount);
            })
            .or_insert(Gas(amount));
    }

    fn exit_dispatch(&mut self, id_exited: ProgramId, value_destination: ProgramId) {
        if let Some((_, balance)) = self.actors.remove(&id_exited) {
            self.mint_to(&value_destination, balance, MintMode::AllowDeath);
        }
    }

    fn message_consumed(&mut self, message_id: MessageId) {
        self.gas_tree
            .consume(message_id)
            .unwrap_or_else(|e| unreachable!("GasTree corrupted! {:?}", e));
    }

    fn send_dispatch(
        &mut self,
        message_id: MessageId,
        dispatch: Dispatch,
        delay: u32,
        reservation: Option<ReservationId>,
    ) {
        if delay > 0 {
            log::debug!("[{message_id}] new delayed dispatch#{}", dispatch.id());

            self.send_delayed_dispatch(dispatch, delay);
            return;
        }

        log::debug!("[{message_id}] new dispatch#{}", dispatch.id());

        let source = dispatch.source();

        if self.is_program(&dispatch.destination()) {
            match (dispatch.gas_limit(), reservation) {
                (Some(gas_limit), None) => self
                    .gas_tree
                    .split_with_value(false, message_id, dispatch.id(), gas_limit)
                    .unwrap_or_else(|e| unreachable!("GasTree corrupted! {:?}", e)),
                (None, None) => self
                    .gas_tree
                    .split(false, message_id, dispatch.id())
                    .unwrap_or_else(|e| unreachable!("GasTree corrupted! {:?}", e)),
                (None, Some(reservation)) => {
                    self.gas_tree
                        .split(false, reservation, dispatch.id())
                        .unwrap_or_else(|e| unreachable!("GasTree corrupted! {:?}", e));
                }
                (Some(_), Some(_)) => unreachable!(
                    "Sending dispatch with gas limit from reservation \
                    is currently unimplemented and there is no way to send such dispatch"
                ),
            }

            self.dispatches.push_back(dispatch.into_stored());
        } else {
            let gas_limit = dispatch.gas_limit().unwrap_or_default();
            let stored_message = dispatch.into_stored().into_parts().1;

            if let Ok(mailbox_msg) = stored_message.clone().try_into() {
                let origin_node = reservation
                    .map(|r| r.into_origin().cast())
                    .unwrap_or(message_id);
                self.gas_tree
                    .cut(origin_node, stored_message.id(), gas_limit)
                    .unwrap_or_else(|e| unreachable!("GasTree corrupted! {:?}", e));

                self.mailbox
                    .insert(mailbox_msg)
                    .unwrap_or_else(|e| unreachable!("Mailbox corrupted! {:?}", e));
            } else {
                log::debug!("A reply message is sent to user: {stored_message:?}");
            };

            self.log.push(stored_message);
        }

        if let Some(reservation) = reservation {
            let has_removed_reservation = self
                .remove_reservation(source, reservation)
                .expect("failed to find genuine_program");
            if !has_removed_reservation {
                unreachable!("Failed to remove reservation {reservation} from {source}");
            }
        }
    }

    fn wait_dispatch(
        &mut self,
        dispatch: StoredDispatch,
        duration: Option<u32>,
        _: MessageWaitedType,
    ) {
        log::debug!("[{}] wait", dispatch.id());

        let dest = dispatch.destination();
        let id = dispatch.id();
        let expected_wake = duration.map(|d| {
            let expected_bn = d + self.blocks_manager.get().height;
            self.task_pool
                .add(expected_bn, ScheduledTask::WakeMessage(dest, id))
                .unwrap_or_else(|e| unreachable!("TaskPool corrupted: {e:?}"));

            expected_bn
        });
        self.wait_list.insert((dest, id), (dispatch, expected_wake));
    }

    fn wake_message(
        &mut self,
        message_id: MessageId,
        program_id: ProgramId,
        awakening_id: MessageId,
        _delay: u32,
    ) {
        log::debug!("[{message_id}] waked message#{awakening_id}");

        if let Some((msg, expected_bn)) = self.wait_list.remove(&(program_id, awakening_id)) {
            self.dispatches.push_back(msg);

            let Some(expected_bn) = expected_bn else {
                return;
            };
            self.task_pool
                .delete(
                    expected_bn,
                    ScheduledTask::WakeMessage(program_id, awakening_id),
                )
                .unwrap_or_else(|e| unreachable!("TaskPool corrupted: {e:?}"));
        }
    }

    #[track_caller]
    fn update_pages_data(
        &mut self,
        program_id: ProgramId,
        pages_data: BTreeMap<GearPage, PageBuf>,
    ) {
        self.update_storage_pages(&program_id, pages_data);
    }

    #[track_caller]
    fn update_allocations(&mut self, program_id: ProgramId, allocations: IntervalsTree<WasmPage>) {
        self.update_genuine_program(program_id, |program| {
            program
                .allocations
                .difference(&allocations)
                .flat_map(IntervalIterator::from)
                .flat_map(|page| page.to_iter())
                .for_each(|ref page| {
                    program.pages_data.remove(page);
                });
            program.allocations = allocations;
        })
        .expect("no genuine program was found");
    }

    #[track_caller]
    fn send_value(&mut self, from: ProgramId, to: Option<ProgramId>, value: Balance) {
        if value == 0 {
            // Nothing to do
            return;
        }
        if let Some(ref to) = to {
            if self.is_program(&from) {
                let mut actors = self.actors.borrow_mut();
                let (_, balance) = actors.get_mut(&from).expect("Can't fail");

                if *balance < value {
                    unreachable!("Actor {:?} balance is less then sent value", from);
                }

                *balance -= value;

                if *balance < crate::EXISTENTIAL_DEPOSIT {
                    *balance = 0;
                }
            }

            self.mint_to(to, value, MintMode::KeepAlive);
        } else {
            self.mint_to(&from, value, MintMode::KeepAlive);
        }
    }

    #[track_caller]
    fn store_new_programs(
        &mut self,
        program_id: ProgramId,
        code_id: CodeId,
        candidates: Vec<(MessageId, ProgramId)>,
    ) {
        if let Some(code) = self.opt_binaries.get(&code_id).cloned() {
            for (init_message_id, candidate_id) in candidates {
                if !self.actors.contains_key(&candidate_id) {
                    let schedule = Schedule::default();
                    let code = Code::try_new(
                        code.clone(),
                        schedule.instruction_weights.version,
                        |module| schedule.rules(module),
                        schedule.limits.stack_height,
                        schedule.limits.data_segments_amount.into(),
                        schedule.limits.table_number.into(),
                    )
                    .expect("Program can't be constructed with provided code");

                    let code_and_id: InstrumentedCodeAndId =
                        CodeAndId::from_parts_unchecked(code, code_id).into();
                    let (code, code_id) = code_and_id.into_parts();

                    self.store_new_actor(
                        candidate_id,
                        Program::Genuine(GenuineProgram {
                            code,
                            code_id,
                            allocations: Default::default(),
                            pages_data: Default::default(),
                            gas_reservation_map: Default::default(),
                        }),
                        Some(init_message_id),
                    );
                    // Transfer the ED from the program-creator to the new program
                    self.send_value(program_id, Some(candidate_id), crate::EXISTENTIAL_DEPOSIT);
                } else {
                    log::debug!("Program with id {candidate_id:?} already exists");
                }
            }
        } else {
            log::debug!("No referencing code with code hash {code_id:?} for candidate programs");
            for (_, invalid_candidate_id) in candidates {
                self.actors
                    .insert(invalid_candidate_id, (TestActor::Dormant, 0));
            }
        }
    }

    #[track_caller]
    fn stop_processing(&mut self, dispatch: StoredDispatch, gas_burned: u64) {
        log::debug!(
            "Not enough gas for processing msg id {}, allowance equals {}, gas tried to burn at least {}",
            dispatch.id(),
            self.gas_allowance,
            gas_burned,
        );

        self.messages_processing_enabled = false;
        self.dispatches.push_front(dispatch);
    }

    fn reserve_gas(
        &mut self,
        message_id: MessageId,
        reservation_id: ReservationId,
        _program_id: ProgramId,
        amount: u64,
        duration: u32,
    ) {
        log::debug!(
            "Reserved: {:?} from {:?} with {:?} for {} blocks",
            amount,
            message_id,
            reservation_id,
            duration
        );

        self.gas_tree
            .reserve_gas(message_id, reservation_id, amount)
            .unwrap_or_else(|e| unreachable!("GasTree corrupted: {:?}", e));
    }

    fn unreserve_gas(
        &mut self,
        reservation_id: ReservationId,
        program_id: ProgramId,
        _expiration: u32,
    ) {
        let has_removed_reservation = self
            .remove_reservation(program_id, reservation_id)
            .expect("failed to find genuine_program");
        if !has_removed_reservation {
            unreachable!("Failed to remove reservation {reservation_id} from {program_id}");
        }
    }

    #[track_caller]
    fn update_gas_reservation(&mut self, program_id: ProgramId, reserver: GasReserver) {
        let block_height = self.blocks_manager.get().height;
        self.update_genuine_program(program_id, |program| {
            program.gas_reservation_map =
                reserver.into_map(block_height, |duration| block_height + duration);
        })
        .expect("no genuine program was found");
    }

    fn system_reserve_gas(&mut self, message_id: MessageId, amount: u64) {
        self.gas_tree
            .system_reserve(message_id, amount)
            .unwrap_or_else(|e| unreachable!("GasTree corrupted: {:?}", e));
    }

    fn system_unreserve_gas(&mut self, message_id: MessageId) {
        self.gas_tree
            .system_unreserve(message_id)
            .unwrap_or_else(|e| unreachable!("GasTree corrupted: {:?}", e));
    }

    fn send_signal(&mut self, message_id: MessageId, destination: ProgramId, code: SignalCode) {
        let reserved = self
            .gas_tree
            .system_unreserve(message_id)
            .unwrap_or_else(|e| unreachable!("GasTree corrupted! {:?}", e));

        if reserved != 0 {
            log::debug!(
                "Send signal issued by {} to {} with {} supply",
                message_id,
                destination,
                reserved
            );

            let trap_signal = SignalMessage::new(message_id, code)
                .into_dispatch(message_id, destination)
                .into_stored();

            self.gas_tree
                .split_with_value(
                    trap_signal.is_reply(),
                    message_id,
                    trap_signal.id(),
                    reserved,
                )
                .unwrap_or_else(|e| unreachable!("GasTree corrupted! {:?}", e));

            self.dispatches.push_back(trap_signal);
        } else {
            log::trace!("Signal wasn't send due to inappropriate supply");
        }
    }

    fn reply_deposit(&mut self, message_id: MessageId, future_reply_id: MessageId, amount: u64) {
        self.gas_tree
            .create_deposit(message_id, future_reply_id, amount)
            .unwrap_or_else(|e| unreachable!("GasTree corrupted! {:?}", e));
    }
}
