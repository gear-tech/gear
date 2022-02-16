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
use common::{
    value_tree::{ConsumeResult, ValueView},
    GasToFeeConverter, Origin, Program, GAS_VALUE_PREFIX, STORAGE_PROGRAM_PREFIX,
};
use core_processor::common::{
    CollectState, DispatchOutcome as CoreDispatchOutcome, JournalHandler, State,
};
use frame_support::{
    storage::PrefixIterator,
    traits::{BalanceStatus, Currency, ExistenceRequirement, ReservableCurrency},
};
use gear_core::{
    memory::PageNumber,
    message::{Dispatch, ExitCode, MessageId},
    program::{Program as NativeProgram, ProgramId},
};
use primitive_types::H256;
use sp_runtime::traits::{UniqueSaturatedInto, Zero};
use sp_std::{collections::btree_map::BTreeMap, marker::PhantomData, prelude::*};

pub struct ExtManager<T: Config, GH: GasHandler = ValueTreeGasHandler> {
    _phantom: PhantomData<T>,
    gas_handler: GH,
}

#[derive(Decode, Encode)]
pub enum HandleKind {
    Init(Vec<u8>),
    Handle(H256),
    Reply(H256, ExitCode),
}

pub trait GasHandler {
    fn spend(&mut self, message_id: H256, amount: u64);
    fn consume(&mut self, message_id: H256) -> ConsumeResult;
    fn split(&mut self, message_id: H256, at: H256, amount: u64);
}

#[derive(Default)]
pub struct ValueTreeGasHandler;

impl GasHandler for ValueTreeGasHandler {
    fn spend(&mut self, message_id: H256, amount: u64) {
        if let Some(mut gas_tree) = ValueView::get(GAS_VALUE_PREFIX, message_id) {
            gas_tree.spend(amount);
        } else {
            log::error!(
                "Message does not have associated gas tree: {:?}",
                message_id
            );
        }
    }

    fn consume(&mut self, message_id: H256) -> ConsumeResult {
        if let Some(gas_tree) = ValueView::get(GAS_VALUE_PREFIX, message_id) {
            gas_tree.consume()
        } else {
            log::error!(
                "Message does not have associated gas tree: {:?}",
                message_id
            );

            ConsumeResult::None
        }
    }

    fn split(&mut self, message_id: H256, at: H256, amount: u64) {
        if let Some(mut gas_tree) = ValueView::get(GAS_VALUE_PREFIX, message_id) {
            let _ = gas_tree.split_off(at, amount);
        } else {
            log::error!(
                "Message does not have associated gas tree: {:?}",
                message_id
            );
        }
    }
}

impl GasHandler for () {
    fn spend(&mut self, _message_id: H256, _amount: u64) {}

    fn consume(&mut self, _message_id: H256) -> ConsumeResult {
        ConsumeResult::None
    }

    fn split(&mut self, _message_id: H256, _at: H256, _amount: u64) {}
}

impl<T, GH> CollectState for ExtManager<T, GH>
where
    T: Config,
    T::AccountId: Origin,
    GH: GasHandler,
{
    fn collect(&self) -> State {
        let programs: BTreeMap<ProgramId, NativeProgram> = PrefixIterator::<H256>::new(
            STORAGE_PROGRAM_PREFIX.to_vec(),
            STORAGE_PROGRAM_PREFIX.to_vec(),
            |key, _| Ok(H256::from_slice(key)),
        )
        .filter_map(|k| self.get_program(k).map(|p| (p.id(), p)))
        .map(|(id, mut prog)| {
            let pages_data = {
                let page_numbers = prog.get_pages().keys().map(|k| k.raw()).collect();
                let data = common::get_program_pages(id.into_origin(), page_numbers)
                    .expect("active program exists, therefore pages do");
                data.into_iter().map(|(k, v)| (k.into(), v)).collect()
            };
            let _ = prog.set_pages(pages_data);
            (id, prog)
        })
        .collect();

        let dispatch_queue = common::dispatch_iter().map(Into::into).collect();

        State {
            dispatch_queue,
            programs,
            ..Default::default()
        }
    }
}

impl<T, GH> Default for ExtManager<T, GH>
where
    T: Config,
    T::AccountId: Origin,
    GH: Default + GasHandler,
{
    fn default() -> Self {
        ExtManager {
            _phantom: PhantomData,
            gas_handler: GH::default(),
        }
    }
}

impl<T, GH> ExtManager<T, GH>
where
    T: Config,
    T::AccountId: Origin,
    GH: GasHandler,
{
    pub fn program_from_code(&self, id: H256, code: Vec<u8>) -> Option<NativeProgram> {
        NativeProgram::new(ProgramId::from_origin(id), code).ok()
    }

    /// # Caution
    /// By calling this function we can't differ whether `None` returned, because
    /// program with `id` doesn't exist or it's terminated
    pub fn get_program(&self, id: H256) -> Option<NativeProgram> {
        common::get_program(id)
            .and_then(|prog_with_status| prog_with_status.try_into_native(id).ok())
    }

    pub fn set_program(&self, program: NativeProgram, message_id: H256) {
        assert!(
            program.get_pages().is_empty(),
            "Must has empty persistent pages, has {:?}",
            program.get_pages()
        );
        let persistent_pages: BTreeMap<u32, Vec<u8>> = program
            .get_pages()
            .iter()
            .map(|(k, v)| (k.raw(), v.as_ref().expect("Must have page data").to_vec()))
            .collect();

        let id = program.id().into_origin();

        let code_hash: H256 = sp_io::hashing::blake2_256(program.code()).into();

        common::set_code(code_hash, program.code());

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

impl<T, GH> JournalHandler for ExtManager<T, GH>
where
    T: Config,
    T::AccountId: Origin,
    GH: GasHandler,
{
    fn message_dispatched(&mut self, outcome: CoreDispatchOutcome) {
        let event = match outcome {
            CoreDispatchOutcome::Success(message_id) => Event::MessageDispatched(DispatchOutcome {
                message_id: message_id.into_origin(),
                outcome: ExecutionResult::Success,
            }),
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

                assert!(
                    common::set_program_terminated_status(program_id).is_ok(),
                    "only active program can cause init failure"
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
            CoreDispatchOutcome::Skip(message_id) => {
                Event::MessageSkipped(message_id.into_origin())
            }
        };

        Pallet::<T>::deposit_event(event);
    }

    fn gas_burned(&mut self, message_id: MessageId, origin: ProgramId, amount: u64) {
        let message_id = message_id.into_origin();

        log::debug!("burned: {:?} from: {:?}", amount, message_id);

        Pallet::<T>::decrease_gas_allowance(amount);

        let charge = T::GasConverter::gas_to_fee(amount);

        self.gas_handler.spend(message_id, amount);

        if let Some(author) = Authorship::<T>::author() {
            let _ = T::Currency::repatriate_reserved(
                &<T::AccountId as Origin>::from_origin(origin.into_origin()),
                &author,
                charge,
                BalanceStatus::Free,
            );
        }
    }

    fn exit_dispatch(&mut self, id_exited: ProgramId, value_destination: ProgramId) {
        let program_id = id_exited.into_origin();
        assert!(
            common::remove_program(program_id).is_ok(),
            "`exit` can be called only from active program"
        );

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

        if let ConsumeResult::RefundExternal(external, gas_left) =
            self.gas_handler.consume(message_id)
        {
            log::debug!("unreserve: {}", gas_left);

            let refund = T::GasConverter::gas_to_fee(gas_left);

            let _ =
                T::Currency::unreserve(&<T::AccountId as Origin>::from_origin(external), refund);
        }
    }

    fn send_dispatch(&mut self, message_id: MessageId, dispatch: Dispatch) {
        let message_id = message_id.into_origin();
        let mut dispatch: common::Dispatch = dispatch.into();

        // TODO reserve call must be infallible in https://github.com/gear-tech/gear/issues/644
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

        if common::program_exists(dispatch.message.dest) {
            self.gas_handler
                .split(message_id, dispatch.message.id, dispatch.message.gas_limit);
            common::queue_dispatch(dispatch);
        } else {
            // Being placed into a user's mailbox means the end of a message life cycle.
            // There can be no further processing whatsoever, hence any gas attempted to be
            // passed along must be returned (i.e. remain in the parent message's value tree).
            if dispatch.message.gas_limit > 0 {
                dispatch.message.gas_limit = 0;
            }
            Pallet::<T>::insert_to_mailbox(dispatch.message.dest, dispatch.message.clone());
            Pallet::<T>::deposit_event(Event::Log(dispatch.message));
        }
    }

    fn wait_dispatch(&mut self, dispatch: Dispatch) {
        let dispatch: common::Dispatch = dispatch.into();

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

        if let Some((dispatch, _)) =
            common::remove_waiting_message(program_id.into_origin(), awakening_id)
        {
            common::queue_dispatch(dispatch);

            Pallet::<T>::deposit_event(Event::RemovedFromWaitList(awakening_id));
        } else {
            log::error!(
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
            let _ = T::Currency::repatriate_reserved(
                &from,
                &to,
                value.unique_saturated_into(),
                BalanceStatus::Free,
            );
        } else {
            log::debug!("Value unreserve of amount {:?} from {:?}", value, from,);
            let from = <T::AccountId as Origin>::from_origin(from);
            T::Currency::unreserve(&from, value.unique_saturated_into());
        }
    }
}
