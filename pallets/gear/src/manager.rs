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

//! Manager which handles results of message processing.
//!
//! Should be mentioned, that if message contains value we have a guarantee that it will be sent further in case of successful execution,
//! or sent back in case execution ends up with an error. This guarantee is reached by the following conditions:
//! 1. **Reserve/unreserve model for transferring values**.
//! Ownership over message value is moved not by simple transfer operation, which decreases **free** balance of sender. That is done by
//! reserving value before message is executed and repatriating reserved in favor of beneficiary in case of successful execution, or unreserving
//! in case of execution resulting in a trap. So, it gives us a guarantee that regardless of the result of message execution, there is **always some
//! value** to perform asset management, i.e move tokens further to the recipient or give back to sender. The guarantee is implemented by using
//! corresponding `pallet_balances` functions (`reserve`, `repatriate_reserved`, `unreserve` along with `transfer`) in `pallet_gear` extrinsics,
//! [`JournalHandler::send_dispatch`] and [`JournalHandler::send_value`] procedures.
//!
//! 2. **Balance sufficiency before adding message with value to the queue**.
//! Before message is added to the queue, sender's balance is checked for having adequate amount of assets to send desired value. For actors, who
//! can sign transactions, these checks are done in extrinsic calls. For programs these checks are done on core backend level during execution. In details,
//! when a message is executed, it has some context, which is set from the pallet level, and a part of the context data is program's actual balance (current balance +
//! value sent within the executing message). So if during execution of the original message some other messages were sent, message send call is followed
//! by program's balance checks. The check gives guarantee that value reservation call in [`JournalHandler::send_dispatch`] for program's messages won't fail,
//! because there is always a sufficient balance for the call.
//!
//! 3. **Messages's value management considers existential deposit rule**.
//! It means that before message with value is added to the queue, value is checked to be in the valid range - `{0} ∪ [existential_deposit; +inf)`. This is
//! crucial for programs. The check gives guarantee that if funds were moved to the program, the program will definitely have an account in `pallet_balances`
//! registry and will be able then to manage these funds. Without this check, program could receive funds, but won't be able to use them.
//!
//! Due to these 3 conditions implemented in `pallet_gear`, we have a guarantee that value management calls, performed by user or program, won't fail.

use crate::{
    Authorship, Config, Event, GearProgramPallet, MailboxOf, Pallet, QueueOf, SentOf, WaitlistOf,
};
use codec::{Decode, Encode};
use common::{
    event::*, storage::*, ActiveProgram, CodeStorage, GasPrice, Origin, Program, ProgramState,
    ValueTree,
};
use core_processor::common::{
    DispatchOutcome as CoreDispatchOutcome, ExecutableActor, JournalHandler,
};
use frame_support::traits::{
    BalanceStatus, Currency, ExistenceRequirement, Get, Imbalance, ReservableCurrency,
};
use gear_core::{
    ids::{CodeId, MessageId, ProgramId},
    memory::{PageBuf, PageNumber},
    message::{Dispatch, ExitCode, StoredDispatch},
    program::Program as NativeProgram,
};
use pallet_gas::Pallet as GasPallet;
use primitive_types::H256;
use sp_runtime::{
    traits::{UniqueSaturatedInto, Zero},
    SaturatedConversion,
};
use sp_std::{
    collections::{btree_map::BTreeMap, btree_set::BTreeSet},
    convert::TryInto,
    marker::PhantomData,
    prelude::*,
};

#[derive(Decode, Encode)]
pub enum HandleKind {
    Init(Vec<u8>),
    Handle(H256),
    Reply(H256, ExitCode),
}

pub struct ExtManager<T: Config> {
    // Ids checked that they are users.
    users: BTreeSet<ProgramId>,
    // Ids checked that they are programs.
    programs: BTreeSet<ProgramId>,
    // Messages dispatches.
    dispatch_statuses: BTreeMap<MessageId, DispatchStatus>,
    _phantom: PhantomData<T>,
}

impl<T: Config> ExtManager<T> {
    pub(crate) fn into_dispatch_statuses(self) -> BTreeMap<MessageId, DispatchStatus> {
        self.dispatch_statuses
    }
}

impl<T: Config> Default for ExtManager<T>
where
    T::AccountId: Origin,
{
    fn default() -> Self {
        ExtManager {
            _phantom: PhantomData,
            users: Default::default(),
            programs: Default::default(),
            dispatch_statuses: Default::default(),
        }
    }
}

impl<T: Config> ExtManager<T>
where
    T::AccountId: Origin,
{
    /// Check if id is program.
    pub fn is_program(&mut self, id: &ProgramId) -> bool {
        // TODO: research how much need to charge for `program_exists` query.
        if self.programs.contains(id) {
            true
        } else if self.users.contains(id) {
            false
        } else if GearProgramPallet::<T>::program_exists(*id) {
            self.programs.insert(*id);
            true
        } else {
            self.users.insert(*id);
            false
        }
    }

    /// Check if id is not program.
    pub fn is_user(&mut self, id: &ProgramId) -> bool {
        !self.is_program(id)
    }

    /// NOTE: By calling this function we can't differ whether `None` returned, because
    /// program with `id` doesn't exist or it's terminated
    pub fn get_executable_actor(&self, id: ProgramId, with_pages: bool) -> Option<ExecutableActor> {
        let active: ActiveProgram = common::get_program(id.into_origin())?.try_into().ok()?;
        let program = {
            let code_id = CodeId::from_origin(active.code_hash);
            let code = T::CodeStorage::get_code(code_id)?;
            NativeProgram::from_parts(
                id,
                code,
                active.allocations,
                matches!(active.state, ProgramState::Initialized),
            )
        };

        let balance = <T as Config>::Currency::free_balance(
            &<T::AccountId as Origin>::from_origin(id.into_origin()),
        )
        .unique_saturated_into();
        let pages_data = if with_pages {
            common::get_program_data_for_pages(id.into_origin(), active.pages_with_data.iter())
                .ok()?
        } else {
            Default::default()
        };

        Some(ExecutableActor {
            program,
            balance,
            pages_data,
        })
    }

    pub fn set_program(&self, program_id: ProgramId, code_id: CodeId, message_id: MessageId) {
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

        common::set_program(program_id.into_origin(), program);
    }
}

impl<T: Config> JournalHandler for ExtManager<T>
where
    T::AccountId: Origin,
{
    fn message_dispatched(
        &mut self,
        message_id: MessageId,
        source: ProgramId,
        outcome: CoreDispatchOutcome,
    ) {
        use CoreDispatchOutcome::*;

        let wake_waiting_init_msgs = |p_id: ProgramId| {
            common::waiting_init_take_messages(p_id)
                .into_iter()
                .for_each(|m_id| {
                    if let Ok((m, _)) = WaitlistOf::<T>::remove(p_id, m_id) {
                        // TODO: update gas limit in ValueTree here.
                        QueueOf::<T>::queue(m)
                            .unwrap_or_else(|e| unreachable!("Message queue corrupted! {:?}", e));
                    } else {
                        log::error!("Cannot find message in wl")
                    }
                })
        };

        let status = match outcome {
            Success => {
                log::trace!("Dispatch outcome success: {:?}", message_id);

                DispatchStatus::Success
            }
            MessageTrap { program_id, trap } => {
                log::trace!("Dispatch outcome trap: {:?}", message_id);

                if let Some(reason) = trap {
                    log::info!(
                        target: "runtime::gear",
                        "🪤 Program {} terminated with a trap: {}",
                        program_id.into_origin(),
                        reason
                    );
                };

                DispatchStatus::Failed
            }
            InitSuccess { program_id, .. } => {
                log::trace!(
                    "Dispatch ({:?}) init success for program {:?}",
                    message_id,
                    program_id
                );

                wake_waiting_init_msgs(program_id);
                common::set_program_initialized(program_id.into_origin());

                // TODO: replace this temporary (zero) value for expiration
                // block number with properly calculated one
                // (issues #646 and #969).
                Pallet::<T>::deposit_event(Event::ProgramChanged {
                    id: program_id,
                    change: ProgramChangeKind::Active {
                        expiration: T::BlockNumber::zero(),
                    },
                });

                DispatchStatus::Success
            }
            InitFailure { program_id, .. } => {
                log::trace!(
                    "Dispatch ({:?}) init failure for program {:?}",
                    message_id,
                    program_id
                );

                // Some messages addressed to the program could be processed
                // in the queue before init message. For example, that could
                // happen when init message had more gas limit then rest block
                // gas allowance, but a dispatch message to the program was
                // dequeued. The other case is async init.
                wake_waiting_init_msgs(program_id);

                common::set_program_terminated_status(program_id.into_origin())
                    .expect("Only active program can cause init failure");

                DispatchStatus::Failed
            }
            CoreDispatchOutcome::NoExecution => {
                log::trace!("Dispatch ({:?}) for program wasn't executed", message_id);

                DispatchStatus::NotExecuted
            }
        };

        if self.is_user(&source) {
            self.dispatch_statuses.insert(message_id, status);
        }
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
        for (message, _) in WaitlistOf::<T>::drain_key(id_exited) {
            QueueOf::<T>::queue(message)
                .unwrap_or_else(|e| unreachable!("Message queue corrupted! {:?}", e));
        }

        let res = common::set_program_terminated_status(id_exited.into_origin());
        assert!(res.is_ok(), "`exit` can be called only from active program");

        let program_account = &<T::AccountId as Origin>::from_origin(id_exited.into_origin());
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
                        log::debug!(
                            "Unreserve balance on message processed: {} to {}",
                            gas_left,
                            external
                        );

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

        if dispatch.value() != 0 {
            <T as Config>::Currency::reserve(
                &<T::AccountId as Origin>::from_origin(dispatch.source().into_origin()),
                dispatch.value().unique_saturated_into(),
            ).unwrap_or_else(|_| unreachable!("Value reservation can't fail due to value sending rules. For more info, see module docs."));
        }

        log::debug!(
            "Sending message {:?} from {:?} with gas limit {:?}",
            dispatch.message(),
            message_id,
            gas_limit,
        );

        if self.is_program(&dispatch.destination()) {
            if let Some(gas_limit) = gas_limit {
                let _ = T::GasHandler::split_with_value(
                    message_id,
                    dispatch.id().into_origin(),
                    gas_limit,
                );
            } else {
                let _ = T::GasHandler::split(message_id, dispatch.id().into_origin());
            }

            QueueOf::<T>::queue(dispatch)
                .unwrap_or_else(|e| unreachable!("Message queue corrupted! {:?}", e));
        } else {
            let message = dispatch.into_parts().1;

            // Being placed into a user's mailbox means the end of a message life cycle.
            // There can be no further processing whatsoever, hence any gas attempted to be
            // passed along must be returned (i.e. remain in the parent message's value tree).

            // TODO: update logic of insertion into mailbox following new
            // flow and deposit appropriate event (issue #1010).
            if MailboxOf::<T>::insert(message.clone()).is_ok() {
                // TODO: replace this temporary (zero) value for expiration
                // block number with properly calculated one
                // (issues #646 and #969).
                Pallet::<T>::deposit_event(Event::UserMessageSent {
                    message,
                    expiration: Some(T::BlockNumber::zero()),
                });
            } else {
                log::error!("Error occurred in mailbox insertion")
            }
        }
    }

    fn wait_dispatch(&mut self, dispatch: StoredDispatch) {
        WaitlistOf::<T>::insert(dispatch.clone())
            .unwrap_or_else(|e| unreachable!("Waitlist corrupted! {:?}", e));

        let origin = if let Some(origin) =
            GasPallet::<T>::get_origin_id(dispatch.id().into_origin())
                .unwrap_or_else(|e| unreachable!("ValueTree corrupted: {:?}!", e))
                .map(MessageId::from_origin)
        {
            if origin == dispatch.id() {
                None
            } else {
                Some(origin)
            }
        } else {
            unreachable!("ValueTree corrupted!")
        };

        // TODO: replace this temporary (zero) value
        // for expiration block number with properly
        // calculated one (issues #646 and #969).
        Pallet::<T>::deposit_event(Event::MessageWaited {
            id: dispatch.id(),
            origin,
            reason: MessageWaitedRuntimeReason::WaitCalled.into_reason(),
            expiration: T::BlockNumber::zero(),
        });
    }

    fn wake_message(
        &mut self,
        message_id: MessageId,
        program_id: ProgramId,
        awakening_id: MessageId,
    ) {
        if let Ok((dispatch, bn)) = WaitlistOf::<T>::remove(program_id, awakening_id) {
            let duration = <frame_system::Pallet<T>>::block_number()
                .saturated_into::<u32>()
                .saturating_sub(bn.saturated_into::<u32>());
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

            let event = Event::MessageWaken {
                id: dispatch.id(),
                reason: MessageWakenRuntimeReason::WakeCalled.into_reason(),
            };

            QueueOf::<T>::queue(dispatch)
                .unwrap_or_else(|e| unreachable!("Message queue corrupted! {:?}", e));

            Pallet::<T>::deposit_event(event);
        } else {
            log::debug!(
                "Attempt to awaken unknown message {:?} from {:?}",
                awakening_id,
                message_id
            );
        }
    }

    fn update_pages_data(
        &mut self,
        program_id: ProgramId,
        pages_data: BTreeMap<PageNumber, PageBuf>,
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
        let value = value.unique_saturated_into();
        if let Some(to) = to.map(|id| id.into_origin()) {
            let from_account = <T::AccountId as Origin>::from_origin(from);
            let to_account = <T::AccountId as Origin>::from_origin(to);
            log::debug!(
                "Sending value of amount {:?} from {:?} to {:?}",
                value,
                from,
                to
            );
            let res = if <T as Config>::Currency::can_reserve(
                &to_account,
                <T as Config>::Currency::minimum_balance(),
            ) {
                // `to` account exists, so we can repatriate reserved value for it.
                <T as Config>::Currency::repatriate_reserved(
                    &from_account,
                    &to_account,
                    value,
                    BalanceStatus::Free,
                )
                .map(|_| ())
            } else {
                let not_freed = <T as Config>::Currency::unreserve(&from_account, value);
                if not_freed != 0u128.unique_saturated_into() {
                    unreachable!("All requested value for unreserve must be freed. For more info, see module docs.");
                }
                <T as Config>::Currency::transfer(
                    &from_account,
                    &to_account,
                    value,
                    ExistenceRequirement::AllowDeath,
                )
            };

            res.unwrap_or_else(|_| {
                unreachable!("Value transfers can't fail. For more info, see module docs.")
            });
        } else {
            let from_account = <T::AccountId as Origin>::from_origin(from);
            let not_freed = <T as Config>::Currency::unreserve(&from_account, value);
            if not_freed == 0u128.unique_saturated_into() {
                log::debug!(
                    "Value amount amount {:?} successfully unreserved from {:?}",
                    value,
                    from,
                );
            } else {
                unreachable!("All requested value for unreserve must be freed. For more info, see module docs.");
            }
        }
    }

    fn store_new_programs(&mut self, code_id: CodeId, candidates: Vec<(ProgramId, MessageId)>) {
        if T::CodeStorage::get_code(code_id).is_some() {
            for (candidate_id, init_message) in candidates {
                if !GearProgramPallet::<T>::program_exists(candidate_id) {
                    self.set_program(candidate_id, code_id, init_message);
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
                self.programs.insert(candidate);
            }
        }
    }

    fn stop_processing(&mut self, dispatch: StoredDispatch, gas_burned: u64) {
        log::debug!(
            "Not enough gas for processing msg id {}, allowance equals {}, gas tried to burn at least {}",
            dispatch.id(),
            GasPallet::<T>::gas_allowance(),
            gas_burned,
        );

        SentOf::<T>::increase();
        GasPallet::<T>::decrease_gas_allowance(gas_burned);
        QueueOf::<T>::requeue(dispatch)
            .unwrap_or_else(|e| unreachable!("Message queue corrupted! {:?}", e));
    }
}
