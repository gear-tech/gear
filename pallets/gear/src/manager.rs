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
    pallet::Reason, Authorship, Config, DispatchOutcome, Event, ExecutionResult, MessageInfo,
    Pallet,
};
use codec::{Decode, Encode};
use common::{DAGBasedLedger, GasPrice, Origin, Program, QueuedDispatch, STORAGE_PROGRAM_PREFIX};
use core_processor::common::{
    CollectState, DispatchOutcome as CoreDispatchOutcome, ExecutableActor, JournalHandler, State,
};
use frame_support::{
    storage::PrefixIterator,
    traits::{BalanceStatus, Currency, ExistenceRequirement, Get, Imbalance, ReservableCurrency},
};
use gear_core::{
    memory::PageNumber,
    message::{Dispatch, ExitCode, MessageId},
    program::{CodeHash, Program as NativeProgram, ProgramId},
};
use primitive_types::H256;
use sp_runtime::{
    traits::{UniqueSaturatedInto, Zero},
    SaturatedConversion,
};
use sp_std::{
    collections::{btree_map::BTreeMap, btree_set::BTreeSet},
    marker::PhantomData,
    prelude::*,
};

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

impl<T: Config> CollectState for ExtManager<T>
where
    T::AccountId: Origin,
{
    fn collect(&self) -> State {
        let actors: BTreeMap<ProgramId, ExecutableActor> = PrefixIterator::<H256>::new(
            STORAGE_PROGRAM_PREFIX.to_vec(),
            STORAGE_PROGRAM_PREFIX.to_vec(),
            |key, _| Ok(H256::from_slice(key)),
        )
        .filter_map(|k| {
            self.get_executable_actor(k)
                .map(|actor| (actor.program.id(), actor))
        })
        .map(|(id, mut actor)| {
            let pages_data = {
                let page_numbers = actor.program.get_pages().keys().map(|k| k.raw()).collect();
                let data = common::get_program_pages(id.into_origin(), page_numbers)
                    .expect("active program exists, therefore pages do");
                data.into_iter().map(|(k, v)| (k.into(), v)).collect()
            };
            let _ = actor.program.set_pages(pages_data);
            (id, actor)
        })
        .collect();

        let dispatch_queue = common::dispatch_iter()
            .map(|dispatch| {
                let gas = T::GasHandler::get_limit(dispatch.message.id)
                    .map(|(gas, _id)| gas)
                    .unwrap_or(0);
                dispatch.into_dispatch(gas)
            })
            .collect();

        State {
            dispatch_queue,
            actors,
            ..Default::default()
        }
    }
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
    pub fn executable_actor_from_code(&self, id: H256, code: Vec<u8>) -> Option<ExecutableActor> {
        NativeProgram::new(ProgramId::from_origin(id), code)
            .ok()
            .map(|program| ExecutableActor {
                program,
                balance: 0,
            })
    }

    /// NOTE: By calling this function we can't differ whether `None` returned, because
    /// program with `id` doesn't exist or it's terminated
    pub fn get_executable_actor(&self, id: H256) -> Option<ExecutableActor> {
        let program = common::get_program(id)
            .and_then(|prog_with_status| prog_with_status.try_into_native(id).ok())?;

        let balance = T::Currency::free_balance(&<T::AccountId as Origin>::from_origin(id))
            .unique_saturated_into();

        Some(ExecutableActor { program, balance })
    }

    pub fn set_program(&self, program: NativeProgram, message_id: H256) {
        assert!(
            program.get_pages().is_empty(),
            "Must has empty persistent pages, has {:?}",
            program.get_pages()
        );
        let code_hash = CodeHash::generate(program.code()).into_origin();
        assert!(
            common::code_exists(code_hash),
            "Program set must be called only when code exists",
        );

        let persistent_pages: BTreeMap<u32, Vec<u8>> = program
            .get_pages()
            .iter()
            .map(|(k, v)| (k.raw(), v.as_ref().expect("Must have page data").to_vec()))
            .collect();

        let id = program.id().into_origin();

        let program = common::ActiveProgram {
            static_pages: program.static_pages(),
            nonce: program.message_nonce(),
            persistent_pages: persistent_pages.keys().copied().collect(),
            code_hash,
            state: common::ProgramState::Uninitialized { message_id },
        };

        common::set_program(id, program, persistent_pages);
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
                            common::queue_dispatch(m);
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
                            common::queue_dispatch(m);
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
                    Reason::Dispatch(reason.as_bytes().to_vec()),
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

        Pallet::<T>::decrease_gas_allowance(amount);

        match T::GasHandler::spend(message_id, amount) {
            Ok(_) => {
                if let Some((_, origin)) = T::GasHandler::get_limit(message_id) {
                    let charge = T::GasPrice::gas_price(amount);
                    if let Some(author) = Authorship::<T>::author() {
                        let _ = T::Currency::repatriate_reserved(
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
            common::queue_dispatch(message);
        }

        let res = common::set_program_terminated_status(program_id);
        assert!(res.is_ok(), "`exit` can be called only from active program");

        let program_account = &<T::AccountId as Origin>::from_origin(program_id);
        let balance = T::Currency::total_balance(program_account);
        if !balance.is_zero() {
            T::Currency::transfer(
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

        if let Some((neg_imbalance, external)) = T::GasHandler::consume(message_id) {
            let gas_left = neg_imbalance.peek();
            log::debug!("Unreserve balance on message processed: {}", gas_left);

            let refund = T::GasPrice::gas_price(gas_left);

            let _ =
                T::Currency::unreserve(&<T::AccountId as Origin>::from_origin(external), refund);
        }
    }

    fn send_dispatch(&mut self, message_id: MessageId, dispatch: Dispatch) {
        let message_id = message_id.into_origin();
        let (gas_limit, dispatch) = QueuedDispatch::without_gas_limit(dispatch);

        if dispatch.message.value != 0
            && T::Currency::reserve(
                &<T::AccountId as Origin>::from_origin(dispatch.message.source),
                dispatch.message.value.unique_saturated_into(),
            )
            .is_err()
        {
            log::debug!(
                "Message (from: {:?}) {:?} will be skipped",
                message_id,
                dispatch.message
            );
            return;
        }

        log::debug!(
            "Sending message {:?} from {:?}",
            dispatch.message,
            message_id
        );

        if common::program_exists(dispatch.message.dest)
            || self
                .marked_destinations
                .contains(&ProgramId::from_origin(dispatch.message.dest))
        {
            if let Some(gas_limit) = gas_limit {
                let _ =
                    T::GasHandler::split_with_value(message_id, *dispatch.message_id(), gas_limit);
            } else {
                let _ = T::GasHandler::split(message_id, *dispatch.message_id());
            }
            common::queue_dispatch(dispatch);
        } else {
            // Being placed into a user's mailbox means the end of a message life cycle.
            // There can be no further processing whatsoever, hence any gas attempted to be
            // passed along must be returned (i.e. remain in the parent message's value tree).
            Pallet::<T>::insert_to_mailbox(dispatch.message.dest, dispatch.message.clone());
            Pallet::<T>::deposit_event(Event::Log(dispatch.message));
        }
    }

    fn wait_dispatch(&mut self, dispatch: Dispatch) {
        let (_gas_limit, dispatch) = QueuedDispatch::without_gas_limit(dispatch);

        let dest = dispatch.message.dest;
        let message_id = dispatch.message.id;

        common::insert_waiting_message(
            dest,
            message_id,
            dispatch.clone(),
            <frame_system::Pallet<T>>::block_number().unique_saturated_into(),
        );

        Pallet::<T>::deposit_event(Event::AddedToWaitList(dispatch.message));
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
                    if let Some((_, origin)) = T::GasHandler::get_limit(message_id.into_origin()) {
                        let charge = T::GasPrice::gas_price(chargeable_amount);
                        if let Some(author) = Authorship::<T>::author() {
                            let _ = T::Currency::repatriate_reserved(
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

            common::queue_dispatch(dispatch);

            Pallet::<T>::deposit_event(Event::RemovedFromWaitList(awakening_id));
        } else {
            log::debug!(
                "Attempt to awaken unknown message {:?} from {:?}",
                awakening_id,
                message_id.into_origin()
            );
        }
    }

    fn update_nonce(&mut self, program_id: ProgramId, nonce: u64) {
        common::set_program_nonce(program_id.into_origin(), nonce);
    }

    fn update_page(
        &mut self,
        program_id: ProgramId,
        page_number: PageNumber,
        data: Option<Vec<u8>>,
    ) {
        let program_id = program_id.into_origin();
        let page_number = page_number.raw();

        let program = common::get_program(program_id)
            .expect("page update guaranteed to be called only for existing and active program");

        if let Program::Active(prog) = program {
            let mut persistent_pages = prog.persistent_pages;

            if let Some(data) = data {
                persistent_pages.insert(page_number);
                common::set_program_page(program_id, page_number, data);
            } else {
                persistent_pages.remove(&page_number);
                common::remove_program_page(program_id, page_number);
            }

            common::set_program_persistent_pages(program_id, persistent_pages);
        };
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
            if T::Currency::can_reserve(&to, T::Currency::minimum_balance()) {
                // `to` account exists, so we can repatriate reserved value for it.
                let _ = T::Currency::repatriate_reserved(
                    &from,
                    &to,
                    value.unique_saturated_into(),
                    BalanceStatus::Free,
                );
            } else {
                T::Currency::unreserve(&from, value.unique_saturated_into());
                let _ = T::Currency::transfer(
                    &from,
                    &to,
                    value.unique_saturated_into(),
                    ExistenceRequirement::AllowDeath,
                );
            }
        } else {
            log::debug!("Value unreserve of amount {:?} from {:?}", value, from,);
            let from = <T::AccountId as Origin>::from_origin(from);
            T::Currency::unreserve(&from, value.unique_saturated_into());
        }
    }

    fn store_new_programs(&mut self, code_hash: CodeHash, candidates: Vec<(ProgramId, MessageId)>) {
        let code_hash = code_hash.inner().into();

        if let Some(code) = common::get_code(code_hash) {
            for (candidate_id, init_message) in candidates {
                if !common::program_exists(candidate_id.into_origin()) {
                    // Code hash for invalid code can't be added to the storage from extrinsics.
                    let new_program = NativeProgram::new(candidate_id, code.clone())
                        .expect("guaranteed to be valid");
                    self.set_program(new_program, init_message.into_origin());
                } else {
                    log::debug!("Program with id {:?} already exists", candidate_id);
                }
            }
        } else {
            log::debug!(
                "No referencing code with code hash {:?} for candidate programs",
                code_hash
            );
            for (candidate, _) in candidates {
                self.marked_destinations.insert(candidate);
            }
        }
    }
}
