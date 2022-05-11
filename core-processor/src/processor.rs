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

use crate::common::ExecutionErrorReason;
use crate::{
    common::{
        DispatchOutcome, DispatchResult, DispatchResultKind, ExecutableActor, ExecutionContext,
        JournalNote,
    },
    configs::{BlockInfo, ExecutionSettings},
    executor,
    ext::ProcessorExt,
};
use alloc::{string::ToString, vec::Vec};
use gear_backend_common::{Environment, IntoExtInfo};
use gear_core::{
    costs::HostFnWeights,
    env::Ext as EnvExt,
    ids::{MessageId, ProgramId},
    message::{
        DispatchKind, ExitCode, IncomingDispatch, ReplyMessage, ReplyPacket, StoredDispatch,
    },
};

enum SuccessfulDispatchResultKind {
    Exit(ProgramId),
    Wait,
    Success,
}

/// Wrapper for processing the [`IncomingDispatch`].
pub struct Processor {
    block_info: BlockInfo,
    existential_deposit: u128,
    origin: ProgramId,
    program_id: ProgramId,
    gas_allowance: u64,
    outgoing_limit: u32,
    host_fn_weights: HostFnWeights,
    forbidden_funcs: Vec<&'static str>,
}

impl Default for Processor {
    fn default() -> Self {
        Self {
            block_info: Default::default(),
            existential_deposit: 0,
            origin: Default::default(),
            program_id: Default::default(),
            gas_allowance: u64::MAX,
            outgoing_limit: 0,
            host_fn_weights: Default::default(),
            forbidden_funcs: Vec::new(),
        }
    }
}

#[allow(clippy::too_many_arguments)]
/// Process program & dispatch for it and return journal for updates.
pub fn process<A: ProcessorExt + EnvExt + IntoExtInfo + 'static, E: Environment<A>>(
    maybe_actor: Option<ExecutableActor>,
    dispatch: IncomingDispatch,
    block_info: BlockInfo,
    existential_deposit: u128,
    origin: ProgramId,
    // TODO: Temporary here for non-executable case. Should be inside executable actor, renamed to Actor.
    program_id: ProgramId,
    gas_allowance: u64,
    outgoing_limit: u32,
    host_fn_weights: HostFnWeights,
) -> Vec<JournalNote> {
    Processor::new()
        .block_info(block_info)
        .existential_deposit(existential_deposit)
        .origin(origin)
        .program_id(program_id)
        .gas_allowance(gas_allowance)
        .outgoing_limit(outgoing_limit)
        .host_fn_weights(host_fn_weights)
        .process::<A, E>(maybe_actor, dispatch)
}

impl Processor {
    /// Create an empty `Processor`.
    pub fn new() -> Self {
        Default::default()
    }

    /// Set the block info (see [`BlockInfo`] for details) and return self.
    pub fn block_info(mut self, block_info: BlockInfo) -> Self {
        self.block_info = block_info;
        self
    }

    /// Set the existential deposit and return self.
    pub fn existential_deposit(mut self, deposit: u128) -> Self {
        self.existential_deposit = deposit;
        self
    }

    /// Set the origin and return self.
    pub fn origin(mut self, origin: ProgramId) -> Self {
        self.origin = origin;
        self
    }

    /// Set the program ID and return self.
    pub fn program_id(mut self, program_id: ProgramId) -> Self {
        self.program_id = program_id;
        self
    }

    /// Set the gas allowance and return self.
    pub fn gas_allowance(mut self, gas_allowance: u64) -> Self {
        self.gas_allowance = gas_allowance;
        self
    }

    /// Set the outgoing limit and return self.
    pub fn outgoing_limit(mut self, outgoing_limit: u32) -> Self {
        self.outgoing_limit = outgoing_limit;
        self
    }

    /// Set the host functions weights and return self.
    pub fn host_fn_weights(mut self, host_fn_weights: HostFnWeights) -> Self {
        self.host_fn_weights = host_fn_weights;
        self
    }

    /// Set the syscalls black list and return self.
    pub fn forbidden_funcs(mut self, forbidden_funcs: Vec<&'static str>) -> Self {
        self.forbidden_funcs = forbidden_funcs;
        self
    }

    /// Process the [`IncomingDispatch`].
    pub fn process<A: ProcessorExt + EnvExt + IntoExtInfo + 'static, E: Environment<A>>(
        self,
        maybe_actor: Option<ExecutableActor>,
        dispatch: IncomingDispatch,
    ) -> Vec<JournalNote> {
        match Self::check_is_executable(maybe_actor, &dispatch) {
            Err(exit_code) => self.process_non_executable(dispatch, exit_code),
            Ok(actor) => self.process_executable::<A, E>(actor, dispatch),
        }
    }

    fn check_is_executable(
        maybe_actor: Option<ExecutableActor>,
        dispatch: &IncomingDispatch,
    ) -> Result<ExecutableActor, ExitCode> {
        maybe_actor
            .map(|a| {
                if a.program.is_initialized() & matches!(dispatch.kind(), DispatchKind::Init) {
                    Err(crate::RE_INIT_EXIT_CODE)
                } else {
                    Ok(a)
                }
            })
            .unwrap_or(Err(crate::UNAVAILABLE_DEST_EXIT_CODE))
    }

    /// Helper function for journal creation in trap/error case
    fn process_error(
        dispatch: IncomingDispatch,
        program_id: ProgramId,
        gas_burned: u64,
        err: Option<ExecutionErrorReason>,
    ) -> Vec<JournalNote> {
        let mut journal = Vec::new();

        let message_id = dispatch.id();
        let origin = dispatch.source();
        let value = dispatch.value();

        journal.push(JournalNote::GasBurned {
            message_id,
            amount: gas_burned,
        });

        if value != 0 {
            // Send back value
            journal.push(JournalNote::SendValue {
                from: origin,
                to: None,
                value,
            });
        }

        if !dispatch.is_reply() || dispatch.exit_code().expect("Checked before") == 0 {
            let id = MessageId::generate_reply(dispatch.id(), crate::ERR_EXIT_CODE);
            let packet = ReplyPacket::system(crate::ERR_EXIT_CODE);
            let message = ReplyMessage::from_packet(id, packet);

            journal.push(JournalNote::SendDispatch {
                message_id,
                dispatch: message.into_dispatch(program_id, dispatch.source(), dispatch.id()),
            });
        }

        let outcome = match dispatch.kind() {
            DispatchKind::Init => DispatchOutcome::InitFailure {
                message_id,
                origin,
                program_id,
                reason: err.map(|e| e.to_string()),
            },
            _ => DispatchOutcome::MessageTrap {
                message_id,
                program_id,
                trap: err.map(|e| e.to_string()),
            },
        };

        journal.push(JournalNote::MessageDispatched(outcome));
        journal.push(JournalNote::MessageConsumed(message_id));

        journal
    }

    /// Helper function for journal creation in success case
    fn process_success(
        kind: SuccessfulDispatchResultKind,
        dispatch_result: DispatchResult,
    ) -> Vec<JournalNote> {
        use SuccessfulDispatchResultKind::*;

        let DispatchResult {
            dispatch,
            generated_dispatches,
            awakening,
            program_candidates,
            gas_amount,
            page_update,
            program_id,
            context_store,
            allocations,
            ..
        } = dispatch_result;

        let mut journal = Vec::new();

        let message_id = dispatch.id();
        let origin = dispatch.source();
        let value = dispatch.value();

        journal.push(JournalNote::GasBurned {
            message_id,
            amount: gas_amount.burned(),
        });

        if value != 0 {
            // Send value further
            journal.push(JournalNote::SendValue {
                from: origin,
                to: Some(program_id),
                value,
            });
        }

        // Must be handled before handling generated dispatches.
        for (code_hash, candidates) in program_candidates {
            journal.push(JournalNote::StoreNewPrograms {
                code_hash,
                candidates,
            });
        }

        for dispatch in generated_dispatches {
            journal.push(JournalNote::SendDispatch {
                message_id,
                dispatch,
            });
        }

        for awakening_id in awakening {
            journal.push(JournalNote::WakeMessage {
                message_id,
                program_id,
                awakening_id,
            });
        }

        for (page_number, data) in page_update {
            journal.push(JournalNote::UpdatePage {
                program_id,
                page_number,
                data,
            })
        }

        if let Some(allocations) = allocations {
            journal.push(JournalNote::UpdateAllocations {
                program_id,
                allocations,
            });
        }

        match kind {
            Exit(value_destination) => {
                journal.push(JournalNote::ExitDispatch {
                    id_exited: program_id,
                    value_destination,
                });
            }
            Wait => {
                journal.push(JournalNote::WaitDispatch(
                    dispatch.into_stored(program_id, context_store),
                ));
            }
            Success => {
                let outcome = match dispatch.kind() {
                    DispatchKind::Init => DispatchOutcome::InitSuccess {
                        message_id,
                        origin,
                        program_id,
                    },
                    _ => DispatchOutcome::Success(message_id),
                };

                journal.push(JournalNote::MessageDispatched(outcome));
                journal.push(JournalNote::MessageConsumed(message_id));
            }
        };

        journal
    }

    /// Process the [`IncomingDispatch`] by the executable (program).
    pub fn process_executable<
        A: ProcessorExt + EnvExt + IntoExtInfo + 'static,
        E: Environment<A>,
    >(
        self,
        actor: ExecutableActor,
        dispatch: IncomingDispatch,
    ) -> Vec<JournalNote> {
        use SuccessfulDispatchResultKind::*;

        let execution_settings = ExecutionSettings::new(
            self.block_info,
            self.existential_deposit,
            self.host_fn_weights,
            self.forbidden_funcs,
        );
        let execution_context = ExecutionContext {
            origin: self.origin,
            gas_allowance: self.gas_allowance,
        };
        let msg_ctx_settings = gear_core::message::ContextSettings::new(0, self.outgoing_limit);

        let program_id = actor.program.id();

        let exec_result = executor::execute_wasm::<A, E>(
            actor,
            dispatch.clone(),
            execution_context,
            execution_settings,
            msg_ctx_settings,
        );

        match exec_result {
            Ok(res) => match res.kind {
                DispatchResultKind::Trap(reason) => Self::process_error(
                    res.dispatch,
                    program_id,
                    res.gas_amount.burned(),
                    reason.map(|e| e.to_string()).map(ExecutionErrorReason::Ext),
                ),
                DispatchResultKind::Success => Self::process_success(Success, res),
                DispatchResultKind::Wait => Self::process_success(Wait, res),
                DispatchResultKind::Exit(value_destination) => {
                    Self::process_success(Exit(value_destination), res)
                }
                DispatchResultKind::GasAllowanceExceed => {
                    Self::process_allowance_exceed(dispatch, program_id, res.gas_amount.burned())
                }
            },
            Err(e) => {
                if e.allowance_exceed {
                    Self::process_allowance_exceed(dispatch, program_id, e.gas_amount.burned())
                } else {
                    Self::process_error(dispatch, program_id, e.gas_amount.burned(), e.reason)
                }
            }
        }
    }

    fn process_allowance_exceed(
        dispatch: IncomingDispatch,
        program_id: ProgramId,
        gas_burned: u64,
    ) -> Vec<JournalNote> {
        let mut journal = Vec::with_capacity(1);

        let (kind, message, opt_context) = dispatch.into_parts();

        let dispatch = StoredDispatch::new(kind, message.into_stored(program_id), opt_context);

        journal.push(JournalNote::StopProcessing {
            dispatch,
            gas_burned,
        });

        journal
    }

    /// Helper function for journal creation in message no execution case.
    fn process_non_executable(
        &self,
        dispatch: IncomingDispatch,
        exit_code: ExitCode,
    ) -> Vec<JournalNote> {
        // Number of notes is predetermined
        let mut journal = Vec::with_capacity(4);

        let message_id = dispatch.id();
        let value = dispatch.value();

        if value != 0 {
            // Send value back
            journal.push(JournalNote::SendValue {
                from: dispatch.source(),
                to: None,
                value,
            });
        }

        // Reply back to the message `source`
        if !dispatch.is_reply() || dispatch.exit_code().expect("Checked before") == 0 {
            let id = MessageId::generate_reply(dispatch.id(), exit_code);
            let packet = ReplyPacket::system(exit_code);
            let message = ReplyMessage::from_packet(id, packet);

            journal.push(JournalNote::SendDispatch {
                message_id,
                dispatch: message.into_dispatch(self.program_id, dispatch.source(), dispatch.id()),
            });
        }

        journal.push(JournalNote::MessageDispatched(
            DispatchOutcome::NoExecution(message_id),
        ));

        journal.push(JournalNote::MessageConsumed(message_id));

        journal
    }
}
