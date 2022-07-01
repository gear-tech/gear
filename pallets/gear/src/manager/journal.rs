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
    manager::{ExtManager, TOL},
    Authorship, Config, Event, GasAllowanceOf, GasHandlerOf, GearProgramPallet, MailboxOf, Pallet,
    QueueOf, SentOf, WaitlistOf,
};
use common::{event::*, storage::*, CodeStorage, GasPrice, GasTree, Origin, Program};
use core_processor::common::{
    DispatchOutcome as CoreDispatchOutcome, ExecutionErrorReason, JournalHandler,
};
use frame_support::traits::{
    BalanceStatus, Currency, ExistenceRequirement, Get, Imbalance, ReservableCurrency,
};
use gear_core::{
    ids::{CodeId, MessageId, ProgramId},
    memory::{PageBuf, PageNumber},
    message::{Dispatch, StoredDispatch},
};
use sp_runtime::{
    traits::{UniqueSaturatedInto, Zero},
    SaturatedConversion,
};
use sp_std::{
    collections::{btree_map::BTreeMap, btree_set::BTreeSet},
    prelude::*,
};

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
                    // TODO: update gas limit in `ValueTree` here (issue #1022).
                    if let Ok((m, _)) = WaitlistOf::<T>::remove(p_id, m_id) {
                        Pallet::<T>::deposit_event(Event::<T>::MessageWoken {
                            id: m_id,
                            reason: MessageWokenSystemReason::ProgramGotInitialized.into_reason(),
                        });

                        QueueOf::<T>::queue(m)
                            .unwrap_or_else(|e| unreachable!("Message queue corrupted! {:?}", e));
                    } else {
                        log::error!("Cannot find message in wl")
                    }
                })
        };

        let status = match outcome {
            Exit { program_id } => {
                log::trace!("Dispatch outcome exit: {:?}", message_id);

                Pallet::<T>::deposit_event(Event::ProgramChanged {
                    id: program_id,
                    change: ProgramChangeKind::Inactive,
                });

                DispatchStatus::Success
            }
            Success => {
                log::trace!("Dispatch outcome success: {:?}", message_id);

                DispatchStatus::Success
            }
            MessageTrap { program_id, trap } => {
                log::trace!("Dispatch outcome trap: {:?}", message_id);
                log::info!(
                    "🪤 Program {} terminated with a trap: {}",
                    program_id.into_origin(),
                    trap
                );

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

        if self.check_user_id(&source) {
            self.dispatch_statuses.insert(message_id, status);
        }
    }

    fn gas_burned(&mut self, message_id: MessageId, amount: u64) {
        log::debug!("Burned: {:?} from: {:?}", amount, message_id);

        GasAllowanceOf::<T>::decrease(amount);

        match GasHandlerOf::<T>::spend(message_id, amount) {
            Ok(_) => {
                match GasHandlerOf::<T>::get_external(message_id) {
                    Ok(maybe_origin) => {
                        if let Some(origin) = maybe_origin {
                            let charge = T::GasPrice::gas_price(amount);
                            if let Some(author) = Authorship::<T>::author() {
                                match <T as Config>::Currency::repatriate_reserved(
                                    &origin,
                                    &author,
                                    charge,
                                    BalanceStatus::Free,
                                ) {
                                    Ok(leftover) => {
                                        if leftover > TOL.unique_saturated_into() {
                                            log::debug!(
                                                target: "essential",
                                                "Reserved funds not fully repatriated from {:?} to 0x{:?}: amount = {:?}, leftover = {:?}",
                                                origin,
                                                author,
                                                charge,
                                                leftover,
                                            );
                                        }
                                    }
                                    Err(e) => {
                                        log::debug!(
                                            target: "essential",
                                            "Failure to repatriate reserves of {:?} from {:?} to 0x{:?}: {:?}",
                                            charge,
                                            origin,
                                            author,
                                            e,
                                        )
                                    }
                                }
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
        // TODO: update gas limit in `ValueTree` here (issue #1022).
        for (message, _) in WaitlistOf::<T>::drain_key(id_exited) {
            QueueOf::<T>::queue(message)
                .unwrap_or_else(|e| unreachable!("Message queue corrupted! {:?}", e));
        }

        let _ = common::waiting_init_take_messages(id_exited);
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
        match GasHandlerOf::<T>::consume(message_id) {
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
                            "Unreserve balance on message processed: {} to {:?}",
                            gas_left,
                            external
                        );

                        let refund = T::GasPrice::gas_price(gas_left);

                        let _ = <T as Config>::Currency::unreserve(&external, refund);
                    }
                }
            }
        }
    }

    fn send_dispatch(&mut self, message_id: MessageId, dispatch: Dispatch) {
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

        if self.check_program_id(&dispatch.destination()) {
            if let Some(gas_limit) = gas_limit {
                let _ = GasHandlerOf::<T>::split_with_value(message_id, dispatch.id(), gas_limit);
            } else {
                let _ = GasHandlerOf::<T>::split(message_id, dispatch.id());
            }

            QueueOf::<T>::queue(dispatch)
                .unwrap_or_else(|e| unreachable!("Message queue corrupted! {:?}", e));
        } else {
            let message = match dispatch.exit_code() {
                Some(0) | None => dispatch.into_parts().1,
                _ => {
                    let message = dispatch.into_parts().1;
                    message
                        .clone()
                        .with_string_payload::<ExecutionErrorReason>()
                        .unwrap_or(message)
                }
            };

            let (mut expiration, mailbox_threshold) = (None, T::MailboxThreshold::get());

            let mut append_message_to_mailbox = |gas_limit: u64| {
                // Being placed into a user's mailbox means the end of a message life cycle.
                // There can be no further processing whatsoever, hence any gas attempted to be
                // passed along must be returned (i.e. remain in the parent message's value tree).

                // TODO: update logic of insertion into mailbox following new
                // flow and deposit appropriate event (issue #1010).
                match MailboxOf::<T>::insert(message.clone()) {
                    Ok(_) => {
                        // TODO: replace this temporary (zero) value for expiration
                        // block number with properly calculated one
                        // (issues #646 and #969).
                        expiration = Some(T::BlockNumber::zero());

                        let _ = GasHandlerOf::<T>::cut(message_id, message.id(), gas_limit);
                    }
                    Err(e) => {
                        log::error!("{:?}", e);
                    }
                }
            };

            if let Some(gas_limit) = gas_limit {
                if gas_limit >= mailbox_threshold {
                    append_message_to_mailbox(gas_limit);
                }
            } else {
                let gas_limit = GasHandlerOf::<T>::get_limit(message_id)
                    .unwrap_or_else(|e| {
                        log::error!("{:?}", e);
                        None
                    })
                    .map(|(g, _)| g)
                    .unwrap_or(0);

                if gas_limit >= mailbox_threshold {
                    append_message_to_mailbox(mailbox_threshold);
                }
            }

            Pallet::<T>::deposit_event(Event::UserMessageSent {
                message,
                expiration,
            });
        }
    }

    fn wait_dispatch(&mut self, dispatch: StoredDispatch) {
        WaitlistOf::<T>::insert(dispatch.clone())
            .unwrap_or_else(|e| unreachable!("Waitlist corrupted! {:?}", e));

        let origin_key = if let Some(key) = GasHandlerOf::<T>::get_origin_key(dispatch.id())
            .unwrap_or_else(|e| unreachable!("ValueTree corrupted: {:?}!", e))
        {
            if key == dispatch.id() {
                None
            } else {
                Some(key)
            }
        } else {
            unreachable!("ValueTree corrupted!")
        };

        // TODO: replace this temporary (zero) value
        // for expiration block number with properly
        // calculated one (issues #646 and #969).
        Pallet::<T>::deposit_event(Event::MessageWaited {
            id: dispatch.id(),
            origin: origin_key,
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

            match GasHandlerOf::<T>::spend(message_id, chargeable_amount) {
                Ok(_) => {
                    match GasHandlerOf::<T>::get_external(message_id) {
                        Ok(maybe_origin) => {
                            if let Some(origin) = maybe_origin {
                                let charge = T::GasPrice::gas_price(chargeable_amount);
                                if let Some(author) = Authorship::<T>::author() {
                                    match <T as Config>::Currency::repatriate_reserved(
                                        &origin,
                                        &author,
                                        charge,
                                        BalanceStatus::Free,
                                    ) {
                                        Ok(leftover) => {
                                            if leftover > TOL.unique_saturated_into() {
                                                log::debug!(
                                                    target: "essential",
                                                    "Reserved funds not fully repatriated from {:?} to 0x{:?}: amount = {:?}, leftover = {:?}",
                                                    origin,
                                                    author,
                                                    charge,
                                                    leftover,
                                                );
                                            }
                                        }
                                        Err(e) => {
                                            log::debug!(
                                                target: "essential",
                                                "Failure to repatriate reserves of {:?} from {:?} to 0x{:?}: {:?}",
                                                charge,
                                                origin,
                                                author,
                                                e,
                                            );
                                        }
                                    }
                                }
                            } else {
                                log::debug!(
                                    target: "essential",
                                    "Failed to get origin of {:?}",
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
                        "Error charging {:?} of gas rent for awakening message {:?}: {:?}",
                        chargeable_amount,
                        message_id,
                        err,
                    );
                }
            }

            let event = Event::MessageWoken {
                id: dispatch.id(),
                reason: MessageWokenRuntimeReason::WakeCalled.into_reason(),
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
        self.state_changes.insert(program_id);
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
                match <T as Config>::Currency::repatriate_reserved(
                    &from_account,
                    &to_account,
                    value,
                    BalanceStatus::Free,
                ) {
                    Ok(leftover) => {
                        if leftover > TOL.unique_saturated_into() {
                            log::debug!(
                                target: "essential",
                                "Reserved funds not fully repatriated from 0x{:?} to 0x{:?}: amount = {:?}, leftover = {:?}",
                                from_account,
                                to_account,
                                value,
                                leftover,
                            );
                        }
                        Ok(())
                    }
                    Err(e) => {
                        log::debug!(
                            target: "essential",
                            "Failure to repatriate reserves of {:?} from 0x{:?} to 0x{:?}: {:?}",
                            value,
                            from_account,
                            to_account,
                            e,
                        );
                        Ok(())
                    }
                }
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
            GasAllowanceOf::<T>::get(),
            gas_burned,
        );

        SentOf::<T>::increase();
        GasAllowanceOf::<T>::decrease(gas_burned);
        QueueOf::<T>::requeue(dispatch)
            .unwrap_or_else(|e| unreachable!("Message queue corrupted! {:?}", e));
    }
}
