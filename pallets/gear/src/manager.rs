// This file is part of Gear.

// Copyright (C) 2021-2022 Gear Technologies Inc.
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

use crate::{
    pallet::Reason, Authorship, Config, DispatchOutcome, Event, ExecutionResult, GearProgramPallet,
    MessageInfo, Pallet,
};
use alloc::collections::BTreeMap;
use codec::{Decode, Encode};
use common::{
    storage::*, ActiveProgram, CodeStorage, GasPrice, Origin, Program, ProgramState, ValueTree,
};
use core_processor::common::{
    DispatchOutcome as CoreDispatchOutcome, ExecutableActor, JournalHandler,
};
use frame_support::traits::{
    BalanceStatus, Currency, ExistenceRequirement, Get, Imbalance, ReservableCurrency,
};
use gear_core::{
    ids::{CodeId, MessageId, ProgramId},
    memory::PageNumber,
    message::{Dispatch, ExitCode, StoredDispatch},
    program::Program as NativeProgram,
};
use pallet_gas::Pallet as GasPallet;
use pallet_gear_messenger::Pallet as MessengerPallet;
use primitive_types::H256;
use sp_runtime::{
    traits::{UniqueSaturatedInto, Zero},
    SaturatedConversion,
};
use sp_std::{collections::btree_set::BTreeSet, convert::TryInto, marker::PhantomData, prelude::*};

pub struct ExtManager<T: Config> {
    // Messages with these destinations will be forcibly pushed to the queue.
    marked_destinations: BTreeSet<ProgramId>,
    _phantom: PhantomData<T>,
}

#[derive(Decode, Encode)]
pub enum HandleKind {
    Init(Vec<u8>),
    Handle(H256),
    Reply(H256, ExitCode),
}

impl<T: Config> Default for ExtManager<T>
where
    T::AccountId: Origin,
{
    fn default() -> Self {
        ExtManager {
            _phantom: PhantomData,
            marked_destinations: Default::default(),
        }
    }
}

impl<T: Config> ExtManager<T>
where
    T::AccountId: Origin,
{
    /// NOTE: By calling this function we can't differ whether `None` returned, because
    /// program with `id` doesn't exist or it's terminated
    pub fn get_executable_actor(&self, id: H256, with_pages: bool) -> Option<ExecutableActor> {
        let active: ActiveProgram = common::get_program(id)?.try_into().ok()?;
        let program = {
            let code_id = CodeId::from_origin(active.code_hash);
            let code = T::CodeStorage::get_code(code_id)?;
            NativeProgram::from_parts(
                ProgramId::from_origin(id),
                code,
                active.allocations,
                matches!(active.state, ProgramState::Initialized),
            )
        };

        let balance =
            <T as Config>::Currency::free_balance(&<T::AccountId as Origin>::from_origin(id))
                .unique_saturated_into();
        let pages_data = if with_pages {
            common::get_program_data_for_pages(id, active.pages_with_data.iter())
        } else {
            Default::default()
        };

        Some(ExecutableActor {
            program,
            balance,
            pages_data,
        })
    }

    pub fn set_program(&self, program_id: ProgramId, code_id: CodeId, message_id: H256) {
        assert!(
            T::CodeStorage::exists(code_id),
            "Program set must be called only when code exists",
        );

        // An empty program has been just constructed: it contains no mem allocations.
        let program = common::ActiveProgram {
            allocations: Default::default(),
            pages_with_data: Default::default(),
            code_hash: code_id.into_origin(),
            state: common::ProgramState::Uninitialized { message_id },
        };

        common::set_program_and_pages_data(program_id.into_origin(), program, Default::default());
    }
}

impl<T: Config> JournalHandler for ExtManager<T>
where
    T::AccountId: Origin,
{
    fn message_dispatched(&mut self, outcome: CoreDispatchOutcome) {
        let event = match outcome {
            CoreDispatchOutcome::Success(message_id) => {
                log::trace!("Dispatch outcome success: {:?}", message_id);

                Event::MessageDispatched(DispatchOutcome {
                    message_id: message_id.into_origin(),
                    outcome: ExecutionResult::Success,
                })
            }
            CoreDispatchOutcome::MessageTrap {
                message_id,
                program_id,
                trap,
            } => {
                let reason = trap
                    .map(|v| {
                        log::info!(
                            target: "runtime::gear",
                            "🪤 Program {} terminated with a trap: {}",
                            program_id.into_origin(),
                            v
                        );
                        v.as_bytes().to_vec()
                    })
                    .unwrap_or_default();

                log::trace!("Dispatch outcome trap: {:?}", message_id);

                Event::MessageDispatched(DispatchOutcome {
                    message_id: message_id.into_origin(),
                    outcome: ExecutionResult::Failure(reason),
                })
            }
            CoreDispatchOutcome::InitSuccess {
                message_id,
                origin,
                program_id,
            } => {
                let program_id = program_id.into_origin();
                let event = Event::InitSuccess(MessageInfo {
                    message_id: message_id.into_origin(),
                    origin: origin.into_origin(),
                    program_id,
                });

                common::waiting_init_take_messages(program_id)
                    .into_iter()
                    .for_each(|m_id| {
                        if let Some((m, _)) = common::remove_waiting_message(program_id, m_id) {
                            <MessengerPallet<T> as Messenger>::Queue::push_back(m).unwrap_or_else(
                                |e| unreachable!("Message queue corrupted! {:?}", e),
                            );
                        }
                    });

                common::set_program_initialized(program_id);

                log::trace!(
                    "Dispatch ({:?}) init success for program {:?}",
                    message_id,
                    program_id
                );

                event
            }
            CoreDispatchOutcome::InitFailure {
                message_id,
                origin,
                program_id,
                reason,
            } => {
                let program_id = program_id.into_origin();
                let origin = origin.into_origin();

                // Some messages addressed to the program could be processed
                // in the queue before init message. For example, that could
                // happen when init message had more gas limit then rest block
                // gas allowance, but a dispatch message to the program was
                // dequeued. The other case is async init.
                common::waiting_init_take_messages(program_id)
                    .into_iter()
                    .for_each(|m_id| {
                        if let Some((m, _)) = common::remove_waiting_message(program_id, m_id) {
                            <MessengerPallet<T> as Messenger>::Queue::push_back(m).unwrap_or_else(
                                |e| unreachable!("Message queue corrupted! {:?}", e),
                            );
                        }
                    });

                let res = common::set_program_terminated_status(program_id);
                assert!(res.is_ok(), "only active program can cause init failure");

                log::trace!(
                    "Dispatch ({:?}) init failure for program {:?}",
                    message_id,
                    program_id
                );

                Event::InitFailure(
                    MessageInfo {
                        message_id: message_id.into_origin(),
                        origin,
                        program_id,
                    },
                    Reason::Dispatch(reason.unwrap_or_default().into_bytes()),
                )
            }
            CoreDispatchOutcome::NoExecution(message_id) => {
                Event::MessageNotExecuted(message_id.into_origin())
            }
        };

        Pallet::<T>::deposit_event(event);
    }

    fn gas_burned(&mut self, message_id: MessageId, amount: u64) {
        let message_id = message_id.into_origin();

        log::debug!("burned: {:?} from: {:?}", amount, message_id);

        GasPallet::<T>::decrease_gas_allowance(amount);

        match T::GasHandler::spend(message_id, amount) {
            Ok(_) => {
                match T::GasHandler::get_origin(message_id) {
                    Ok(maybe_origin) => {
                        if let Some(origin) = maybe_origin {
                            let charge = T::GasPrice::gas_price(amount);
                            if let Some(author) = Authorship::<T>::author() {
                                let _ = <T as Config>::Currency::repatriate_reserved(
                                    &<T::AccountId as Origin>::from_origin(origin),
                                    &author,
                                    charge,
                                    BalanceStatus::Free,
                                );
                            }
                        } else {
                            log::debug!(
                                target: "essential",
                                "Failed to get limit of {:?}",
                                message_id,
                            );
                        }
                    }
                    Err(_err) => {
                        // We only can get an error here if the gas tree is invalidated
                        // TODO: handle appropriately
                        unreachable!("Can never happen unless gas tree corrupted");
                    }
                }
            }
            Err(err) => {
                log::debug!(
                    "Error spending {:?} gas for message_id {:?}: {:?}",
                    amount,
                    message_id,
                    err
                )
            }
        }
    }

    fn exit_dispatch(&mut self, id_exited: ProgramId, value_destination: ProgramId) {
        let program_id = id_exited.into_origin();

        for message in common::remove_program_waitlist(program_id) {
            <MessengerPallet<T> as Messenger>::Queue::push_back(message)
                .unwrap_or_else(|e| unreachable!("Message queue corrupted! {:?}", e));
        }

        let res = common::set_program_terminated_status(program_id);
        assert!(res.is_ok(), "`exit` can be called only from active program");

        let program_account = &<T::AccountId as Origin>::from_origin(program_id);
        let balance = <T as Config>::Currency::total_balance(program_account);
        if !balance.is_zero() {
            <T as Config>::Currency::transfer(
                program_account,
                &<T::AccountId as Origin>::from_origin(value_destination.into_origin()),
                balance,
                ExistenceRequirement::AllowDeath,
            )
            .expect("balance is not zero; should not fail");
        }
    }

    fn message_consumed(&mut self, message_id: MessageId) {
        let message_id = message_id.into_origin();

        match T::GasHandler::consume(message_id) {
            Err(_e) => {
                // We only can get an error here if the gas tree is invalidated
                // TODO: handle appropriately
                unreachable!("Can never happen unless gas tree corrupted");
            }
            Ok(maybe_outcome) => {
                if let Some((neg_imbalance, external)) = maybe_outcome {
                    let gas_left = neg_imbalance.peek();

                    if gas_left > 0 {
                        log::debug!("Unreserve balance on message processed: {}", gas_left);

                        let refund = T::GasPrice::gas_price(gas_left);

                        let _ = <T as Config>::Currency::unreserve(
                            &<T::AccountId as Origin>::from_origin(external),
                            refund,
                        );
                    }
                }
            }
        }
    }

    fn send_dispatch(&mut self, message_id: MessageId, dispatch: Dispatch) {
        let message_id = message_id.into_origin();
        let gas_limit = dispatch.gas_limit();
        let dispatch = dispatch.into_stored();

        if dispatch.value() != 0
            && <T as Config>::Currency::reserve(
                &<T::AccountId as Origin>::from_origin(dispatch.source().into_origin()),
                dispatch.value().unique_saturated_into(),
            )
            .is_err()
        {
            log::debug!(
                "Message (from: {:?}) {:?} will be skipped",
                message_id,
                dispatch.message()
            );
            return;
        }

        log::debug!(
            "Sending message {:?} from {:?}",
            dispatch.message(),
            message_id
        );

        if GearProgramPallet::<T>::program_exists(dispatch.destination().into_origin())
            || self.marked_destinations.contains(&dispatch.destination())
        {
            if let Some(gas_limit) = gas_limit {
                let _ = T::GasHandler::split_with_value(
                    message_id,
                    dispatch.id().into_origin(),
                    gas_limit,
                );
            } else {
                let _ = T::GasHandler::split(message_id, dispatch.id().into_origin());
            }

            <MessengerPallet<T> as Messenger>::Queue::push_back(dispatch)
                .unwrap_or_else(|e| unreachable!("Message queue corrupted! {:?}", e));
        } else {
            let message = dispatch.into_parts().1;

            // Being placed into a user's mailbox means the end of a message life cycle.
            // There can be no further processing whatsoever, hence any gas attempted to be
            // passed along must be returned (i.e. remain in the parent message's value tree).
            Pallet::<T>::insert_to_mailbox(message.destination().into_origin(), message.clone());

            Pallet::<T>::deposit_event(Event::Log(message));
        }
    }

    fn wait_dispatch(&mut self, dispatch: StoredDispatch) {
        common::insert_waiting_message(
            dispatch.destination().into_origin(),
            dispatch.id().into_origin(),
            dispatch.clone(),
            <frame_system::Pallet<T>>::block_number().unique_saturated_into(),
        );

        Pallet::<T>::deposit_event(Event::AddedToWaitList(dispatch));
    }

    fn wake_message(
        &mut self,
        message_id: MessageId,
        program_id: ProgramId,
        awakening_id: MessageId,
    ) {
        let awakening_id = awakening_id.into_origin();

        if let Some((dispatch, bn)) =
            common::remove_waiting_message(program_id.into_origin(), awakening_id)
        {
            let duration = <frame_system::Pallet<T>>::block_number()
                .saturated_into::<u32>()
                .saturating_sub(bn);
            let chargeable_amount = T::WaitListFeePerBlock::get().saturating_mul(duration.into());

            match T::GasHandler::spend(message_id.into_origin(), chargeable_amount) {
                Ok(_) => {
                    match T::GasHandler::get_origin(message_id.into_origin()) {
                        Ok(maybe_origin) => {
                            if let Some(origin) = maybe_origin {
                                let charge = T::GasPrice::gas_price(chargeable_amount);
                                if let Some(author) = Authorship::<T>::author() {
                                    let _ = <T as Config>::Currency::repatriate_reserved(
                                        &<T::AccountId as Origin>::from_origin(origin),
                                        &author,
                                        charge,
                                        BalanceStatus::Free,
                                    );
                                }
                            } else {
                                log::debug!(
                                    target: "essential",
                                    "Failed to get limit of {:?}",
                                    message_id,
                                );
                            }
                        }
                        Err(_err) => {
                            // We only can get an error here if the gas tree is invalidated
                            // TODO: handle appropriately
                            unreachable!("Can never happen unless gas tree corrupted");
                        }
                    }
                }
                Err(err) => {
                    log::debug!(
                        target: "essential",
                        "Error charging {:?} gas rent of getting out of waitlist for message_id {:?}: {:?}",
                        chargeable_amount,
                        message_id,
                        err,
                    )
                }
            };

            <MessengerPallet<T> as Messenger>::Queue::push_back(dispatch)
                .unwrap_or_else(|e| unreachable!("Message queue corrupted! {:?}", e));

            Pallet::<T>::deposit_event(Event::RemovedFromWaitList(awakening_id));
        } else {
            log::debug!(
                "Attempt to awaken unknown message {:?} from {:?}",
                awakening_id,
                message_id.into_origin()
            );
        }
    }

    fn update_pages_data(
        &mut self,
        program_id: ProgramId,
        pages_data: BTreeMap<PageNumber, Vec<u8>>,
    ) {
        let program_id = program_id.into_origin();
        let program = common::get_program(program_id)
            .expect("page update guaranteed to be called only for existing and active program");
        if let Program::Active(mut program) = program {
            for (page, data) in pages_data {
                common::set_program_page_data(program_id, page, data);
                program.pages_with_data.insert(page);
            }
            common::set_program(program_id, program);
        }
    }

    fn update_allocations(
        &mut self,
        program_id: ProgramId,
        allocations: BTreeSet<gear_core::memory::WasmPageNumber>,
    ) {
        let program_id = program_id.into_origin();
        let program = common::get_program(program_id)
            .expect("page update guaranteed to be called only for existing and active program");
        if let Program::Active(mut program) = program {
            let removed_pages = program.allocations.difference(&allocations);
            for page in removed_pages.flat_map(|p| p.to_gear_pages_iter()) {
                if program.pages_with_data.remove(&page) {
                    common::remove_program_page_data(program_id, page);
                }
            }
            program.allocations = allocations;
            common::set_program(program_id, program);
        }
    }

    fn send_value(&mut self, from: ProgramId, to: Option<ProgramId>, value: u128) {
        let from = from.into_origin();
        if let Some(to) = to {
            let to = to.into_origin();
            log::debug!(
                "Value send of amount {:?} from {:?} to {:?}",
                value,
                from,
                to
            );
            let from = <T::AccountId as Origin>::from_origin(from);
            let to = <T::AccountId as Origin>::from_origin(to);
            if <T as Config>::Currency::can_reserve(&to, <T as Config>::Currency::minimum_balance())
            {
                // `to` account exists, so we can repatriate reserved value for it.
                let _ = <T as Config>::Currency::repatriate_reserved(
                    &from,
                    &to,
                    value.unique_saturated_into(),
                    BalanceStatus::Free,
                );
            } else {
                <T as Config>::Currency::unreserve(&from, value.unique_saturated_into());
                let _ = <T as Config>::Currency::transfer(
                    &from,
                    &to,
                    value.unique_saturated_into(),
                    ExistenceRequirement::AllowDeath,
                );
            }
        } else {
            log::debug!("Value unreserve of amount {:?} from {:?}", value, from,);
            let from = <T::AccountId as Origin>::from_origin(from);
            <T as Config>::Currency::unreserve(&from, value.unique_saturated_into());
        }
    }

    fn store_new_programs(&mut self, code_id: CodeId, candidates: Vec<(ProgramId, MessageId)>) {
        if T::CodeStorage::get_code(code_id).is_some() {
            for (candidate_id, init_message) in candidates {
                if !GearProgramPallet::<T>::program_exists(candidate_id.into_origin()) {
                    self.set_program(candidate_id, code_id, init_message.into_origin());
                } else {
                    log::debug!("Program with id {:?} already exists", candidate_id);
                }
            }
        } else {
            log::debug!(
                "No referencing code with code hash {:?} for candidate programs",
                code_id
            );
            for (candidate, _) in candidates {
                self.marked_destinations.insert(candidate);
            }
        }
    }

    fn stop_processing(&mut self, dispatch: StoredDispatch, gas_burned: u64) {
        log::debug!(
            "Not enought gas for processing msg id {}, allowance equals {}, gas tried to burn at least {}",
            dispatch.id(),
            GasPallet::<T>::gas_allowance(),
            gas_burned,
        );

        <MessengerPallet<T> as Messenger>::Sent::increase();
        GasPallet::<T>::decrease_gas_allowance(gas_burned);
        <MessengerPallet<T> as Messenger>::Queue::push_front(dispatch)
            .unwrap_or_else(|e| unreachable!("Message queue corrupted! {:?}", e));
    }
}
