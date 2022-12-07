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

use super::*;
use gear_wasm_instrument::syscalls::SysCallName;

impl<T: Config> pallet::Pallet<T>
where
    T::AccountId: Origin,
{
    pub(crate) fn calculate_gas_info_impl(
        source: H256,
        kind: HandleKind,
        initial_gas: u64,
        payload: Vec<u8>,
        value: u128,
        allow_other_panics: bool,
    ) -> Result<GasInfo, Vec<u8>> {
        let account = <T::AccountId as Origin>::from_origin(source);

        let balance = CurrencyOf::<T>::free_balance(&account);
        let max_balance: BalanceOf<T> =
            T::GasPrice::gas_price(initial_gas) + value.unique_saturated_into();
        CurrencyOf::<T>::deposit_creating(&account, max_balance.saturating_sub(balance));

        let who = frame_support::dispatch::RawOrigin::Signed(account);
        let value: BalanceOf<T> = value.unique_saturated_into();

        QueueOf::<T>::clear();

        match kind {
            HandleKind::Init(code) => {
                let salt = b"calculate_gas_salt".to_vec();
                Self::upload_program(who.into(), code, salt, payload, initial_gas, value).map_err(
                    |e| format!("Internal error: upload_program failed with '{e:?}'").into_bytes(),
                )?;
            }
            HandleKind::InitByHash(code_id) => {
                let salt = b"calculate_gas_salt".to_vec();
                Self::create_program(who.into(), code_id, salt, payload, initial_gas, value)
                    .map_err(|e| {
                        format!("Internal error: create_program failed with '{e:?}'").into_bytes()
                    })?;
            }
            HandleKind::Handle(destination) => {
                Self::send_message(who.into(), destination, payload, initial_gas, value).map_err(
                    |e| format!("Internal error: send_message failed with '{e:?}'").into_bytes(),
                )?;
            }
            HandleKind::Reply(reply_to_id, _status_code) => {
                Self::send_reply(who.into(), reply_to_id, payload, initial_gas, value).map_err(
                    |e| format!("Internal error: send_reply failed with '{e:?}'").into_bytes(),
                )?;
            }
            HandleKind::Signal(_signal_from, _status_code) => {
                return Err("Gas calculation for `handle_signal` is not supported"
                    .as_bytes()
                    .to_vec());
            }
        };

        let (main_message_id, main_program_id) = QueueOf::<T>::iter()
            .next()
            .ok_or_else(|| b"Internal error: failed to get last message".to_vec())
            .and_then(|queued| {
                queued
                    .map_err(|_| b"Internal error: failed to retrieve queued dispatch".to_vec())
                    .map(|dispatch| (dispatch.id(), dispatch.destination()))
            })?;

        let block_info = BlockInfo {
            height: Self::block_number().unique_saturated_into(),
            timestamp: <pallet_timestamp::Pallet<T>>::get().unique_saturated_into(),
        };

        let existential_deposit = CurrencyOf::<T>::minimum_balance().unique_saturated_into();

        let schedule = T::Schedule::get();

        let allocations_config = AllocationsConfig {
            max_pages: schedule.limits.memory_pages.into(),
            init_cost: schedule.memory_weights.initial_cost,
            alloc_cost: schedule.memory_weights.allocation_cost,
            mem_grow_cost: schedule.memory_weights.grow_cost,
            load_page_cost: schedule.memory_weights.load_cost,
        };

        let block_config = BlockConfig {
            block_info,
            allocations_config,
            existential_deposit,
            outgoing_limit: T::OutgoingLimit::get(),
            host_fn_weights: schedule.host_fn_weights.into_core(),
            forbidden_funcs: [SysCallName::GasAvailable].into(),
            mailbox_threshold: T::MailboxThreshold::get(),
            waitlist_cost: CostsPerBlockOf::<T>::waitlist(),
            reserve_for: CostsPerBlockOf::<T>::reserve_for().unique_saturated_into(),
            reservation: CostsPerBlockOf::<T>::reservation().unique_saturated_into(),
            read_cost: DbWeightOf::<T>::get().reads(1).ref_time(),
            write_cost: DbWeightOf::<T>::get().writes(1).ref_time(),
            write_per_byte_cost: schedule.db_write_per_byte,
            read_per_byte_cost: schedule.db_read_per_byte,
            module_instantiation_byte_cost: schedule.module_instantiation_per_byte,
            max_reservations: T::ReservationsLimit::get(),
        };

        let mut min_limit = 0;
        let mut reserved = 0;
        let mut burned = 0;
        let mut may_be_returned = 0;

        let mut ext_manager = ExtManager::<T>::default();

        while let Some(queued_dispatch) =
            QueueOf::<T>::dequeue().map_err(|_| b"MQ storage corrupted".to_vec())?
        {
            let actor_id = queued_dispatch.destination();

            let actor = ext_manager
                .get_actor(actor_id)
                .ok_or_else(|| b"Program not found in the storage".to_vec())?;

            let dispatch_id = queued_dispatch.id();
            let gas_limit = GasHandlerOf::<T>::get_limit(dispatch_id)
                .map_err(|_| b"Internal error: unable to get gas limit".to_vec())?;

            let precharged_dispatch = match core_processor::precharge(
                &block_config,
                GasAllowanceOf::<T>::get(),
                queued_dispatch.into_incoming(gas_limit),
                actor_id,
            ) {
                PrechargeResult::Ok(d) => d,
                PrechargeResult::Error(_) => {
                    return Err(b"Failed to charge message for Program".to_vec());
                }
            };

            let subsequent_execution = ext_manager.program_pages_loaded(&actor_id);
            let message_execution_context = MessageExecutionContext {
                actor,
                precharged_dispatch,
                origin: ProgramId::from_origin(source),
                subsequent_execution,
            };

            let subsequent_execution = !subsequent_execution && actor_id == main_program_id;
            let may_be_returned_context = subsequent_execution.then(|| MessageExecutionContext {
                subsequent_execution,
                ..message_execution_context.clone()
            });

            let journal = match core_processor::prepare(&block_config, message_execution_context) {
                PrepareResult::Ok(context) => {
                    let memory_pages = match Self::get_and_track_memory_pages(
                        &mut ext_manager,
                        actor_id,
                        &context.actor_data().pages_with_data,
                    ) {
                        None => continue,
                        Some(m) => m,
                    };

                    let code = Self::get_code(context.actor_data().code_id, actor_id)
                        .unwrap_or_else(|| unreachable!("Program exists so do code"));

                    may_be_returned += may_be_returned_context
                        .map(|c| {
                            let burned = match core_processor::prepare(&block_config, c) {
                                PrepareResult::Ok(context) => context.gas_counter().burned(),
                                _ => context.gas_counter().burned(),
                            };

                            context.gas_counter().burned() - burned
                        })
                        .unwrap_or(0);

                    let (random, bn) = T::Randomness::random(dispatch_id.as_ref());
                    core_processor::process::<Ext, ExecutionEnvironment>(
                        &block_config,
                        (context, actor_id, code).into(),
                        (random.encode(), bn.unique_saturated_into()),
                        memory_pages,
                    )
                }
                PrepareResult::WontExecute(journal) | PrepareResult::Error(journal) => journal,
            };

            let get_main_limit = || GasHandlerOf::<T>::get_limit(main_message_id).ok();

            let get_origin_msg_of = |msg_id| {
                GasHandlerOf::<T>::get_origin_key(msg_id)
                    .map_err(|_| b"Internal error: unable to get origin key".to_vec())
            };
            let from_main_chain =
                |msg_id| get_origin_msg_of(msg_id).map(|v| v == main_message_id.into());

            // TODO: Check whether we charge gas fee for submitting code after #646
            for note in journal {
                core_processor::handle_journal(vec![note.clone()], &mut ext_manager);

                if let Some(remaining_gas) = get_main_limit() {
                    min_limit = min_limit.max(initial_gas.saturating_sub(remaining_gas));
                }

                match note {
                    JournalNote::SendDispatch { dispatch, .. } => {
                        let destination =
                            T::AccountId::from_origin(dispatch.destination().into_origin());
                        if MailboxOf::<T>::contains(&destination, &dispatch.id())
                            && from_main_chain(dispatch.id())?
                        {
                            let gas_limit = dispatch
                                .gas_limit()
                                .or_else(|| GasHandlerOf::<T>::get_limit(dispatch.id()).ok())
                                .ok_or_else(|| {
                                    b"Internal error: unable to get gas limit after execution"
                                        .to_vec()
                                })?;

                            reserved = reserved.saturating_add(gas_limit);
                        }
                    }

                    JournalNote::GasBurned { amount, message_id } => {
                        if from_main_chain(message_id)? {
                            burned = burned.saturating_add(amount);
                        }
                    }

                    JournalNote::MessageDispatched {
                        outcome: CoreDispatchOutcome::MessageTrap { trap, program_id },
                        ..
                    } if program_id == main_program_id || !allow_other_panics => {
                        return Err(format!("Program terminated with a trap: {trap}").into_bytes());
                    }

                    _ => (),
                }
            }
        }

        let waited = WaitlistOf::<T>::contains(&main_program_id, &main_message_id);

        Ok(GasInfo {
            min_limit,
            reserved,
            burned,
            may_be_returned,
            waited,
        })
    }
}
