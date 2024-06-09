use alloc::collections::BTreeMap;
use core_processor::common::{DispatchOutcome, JournalHandler};
use gear_core::{
    ids::ProgramId,
    memory::PageBuf,
    message::Dispatch,
    pages::{numerated::tree::IntervalsTree, GearPage, WasmPage},
    reservation::GasReserver,
};
use gear_core_errors::SignalCode;
use gprimitives::{MessageId, ReservationId};

use crate::{
    receipts::Receipt,
    state::{InitStatus, Storage},
    DispatchExecutionContext, ProgramContext, RuntimeInterface,
};

fn remove_reservation_map(program_context: &mut ProgramContext) {
    let ProgramContext::Executable(ctx) = program_context else {
        unreachable!("Remove reservation map on non-executable program");
    };
    if ctx.gas_reservation_map.is_empty() {
        return;
    }
    todo!("Return reserved gas");
}

impl<S: Storage, RI: RuntimeInterface<S>> JournalHandler for DispatchExecutionContext<'_, S, RI> {
    fn message_dispatched(
        &mut self,
        message_id: MessageId,
        source: ProgramId,
        outcome: DispatchOutcome,
    ) {
        match outcome {
            DispatchOutcome::Exit { .. } => todo!(),
            DispatchOutcome::InitSuccess { program_id } => {
                log::trace!("Dispatch {message_id:?} init success for program {program_id:?}");
                if program_id != self.program_id {
                    unreachable!("Program ID mismatch");
                }
                let ProgramContext::Executable(ctx) = self.program_context else {
                    unreachable!("Init success on non-executable program");
                };
                ctx.status = InitStatus::Initialized;
            }
            DispatchOutcome::InitFailure {
                program_id,
                origin,
                reason,
            } => {
                log::trace!(
                    "Dispatch {message_id:?} init failure for program {program_id:?}: {reason}"
                );
                if program_id != self.program_id {
                    unreachable!("Program ID mismatch");
                }
                remove_reservation_map(self.program_context);
                *self.program_context = ProgramContext::Terminated(origin);
            }
            DispatchOutcome::MessageTrap { .. } => todo!(),
            DispatchOutcome::Success => {
                // TODO: Implement
            }
            DispatchOutcome::NoExecution => {
                todo!()
            }
        }
    }

    fn gas_burned(&mut self, message_id: MessageId, amount: u64) {
        if self.dispatch.id != message_id {
            unreachable!("Message ID mismatch");
        }
        self.dispatch.gas_limit =
            self.dispatch
                .gas_limit
                .checked_sub(amount)
                .unwrap_or_else(|| {
                    unreachable!("Gas limit underflow");
                });
    }

    fn exit_dispatch(&mut self, id_exited: ProgramId, value_destination: ProgramId) {
        if self.program_id != id_exited {
            unreachable!("Program ID mismatch");
        }
        *self.program_context = ProgramContext::Exited(value_destination);
    }

    fn message_consumed(&mut self, message_id: MessageId) {
        // TODO: Implement
    }

    fn send_dispatch(
        &mut self,
        message_id: MessageId,
        dispatch: Dispatch,
        delay: u32,
        reservation: Option<ReservationId>,
    ) {
        if reservation.is_some() {
            todo!()
        }
        if delay != 0 {
            todo!()
        }
        self.receipts.push(Receipt::SendDispatch {
            id: message_id,
            dispatch,
        });
    }

    fn wait_dispatch(
        &mut self,
        dispatch: gear_core::message::StoredDispatch,
        duration: Option<u32>,
        waited_type: gear_core::message::MessageWaitedType,
    ) {
        todo!()
    }

    fn wake_message(
        &mut self,
        message_id: MessageId,
        program_id: ProgramId,
        awakening_id: MessageId,
        delay: u32,
    ) {
        todo!()
    }

    fn update_pages_data(
        &mut self,
        program_id: ProgramId,
        pages_data: BTreeMap<GearPage, PageBuf>,
    ) {
        let ProgramContext::Executable(ctx) = self.program_context else {
            unreachable!("Update pages data on non-executable program");
        };
        if program_id != self.program_id {
            unreachable!("Program ID mismatch");
        }
        for (page, data) in pages_data {
            let hash = self.ri.storage().write_page_data(data);
            ctx.pages_map.insert(page, hash);
        }
    }

    fn update_allocations(&mut self, program_id: ProgramId, allocations: IntervalsTree<WasmPage>) {
        let ProgramContext::Executable(ctx) = self.program_context else {
            unreachable!("Update allocations on non-executable program");
        };
        if program_id != self.program_id {
            unreachable!("Program ID mismatch");
        }
        for page in ctx
            .allocations
            .difference(&allocations)
            .flat_map(|i| i.iter())
            .flat_map(|p| p.to_iter())
        {
            let _ = ctx.pages_map.remove(&page);
        }
        ctx.allocations = allocations;
    }

    fn send_value(&mut self, from: ProgramId, to: Option<ProgramId>, value: u128) {
        let to = to.unwrap_or(from);
        match self.program_context {
            ProgramContext::Executable(ctx) if self.program_id == to => {
                ctx.balance.saturating_add(value);
            }
            _ => {
                self.receipts.push(Receipt::SendValue { from, to, value });
            }
        };
    }

    fn store_new_programs(
        &mut self,
        code_id: gprimitives::CodeId,
        candidates: Vec<(MessageId, ProgramId)>,
    ) {
        todo!()
    }

    fn stop_processing(&mut self, dispatch: gear_core::message::StoredDispatch, gas_burned: u64) {
        todo!()
    }

    fn reserve_gas(
        &mut self,
        message_id: MessageId,
        reservation_id: ReservationId,
        program_id: ProgramId,
        amount: u64,
        bn: u32,
    ) {
        todo!()
    }

    fn unreserve_gas(
        &mut self,
        reservation_id: ReservationId,
        program_id: ProgramId,
        expiration: u32,
    ) {
        todo!()
    }

    fn update_gas_reservation(&mut self, _program_id: ProgramId, _reserver: GasReserver) {
        // TODO: Implement
    }

    fn system_reserve_gas(&mut self, message_id: MessageId, amount: u64) {
        todo!()
    }

    fn system_unreserve_gas(&mut self, message_id: MessageId) {
        todo!()
    }

    fn send_signal(&mut self, message_id: MessageId, destination: ProgramId, code: SignalCode) {
        todo!()
    }

    fn reply_deposit(&mut self, message_id: MessageId, future_reply_id: MessageId, amount: u64) {
        todo!()
    }
}
