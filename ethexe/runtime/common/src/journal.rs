use core::num::NonZeroU32;

use crate::{
    state::{
        self, ActiveProgram, ComplexStorage, Dispatch, HashAndLen, MaybeHash, Program,
        ProgramState, Storage,
    },
    InBlockTransitions,
};
use alloc::{collections::BTreeMap, vec, vec::Vec};
use anyhow::Result;
use core_processor::{
    common::{DispatchOutcome, JournalHandler},
    configs::BlockInfo,
};
use ethexe_common::{db::ScheduledTask, router::OutgoingMessage};
use gear_core::{
    ids::ProgramId,
    memory::PageBuf,
    message::{
        Dispatch as CoreDispatch, Message, MessageWaitedType, Payload, StoredDispatch,
        StoredMessage,
    },
    pages::{numerated::tree::IntervalsTree, GearPage, WasmPage},
    program::ProgramState as InitStatus,
    reservation::GasReserver,
};
use gear_core_errors::SignalCode;
use gprimitives::{ActorId, CodeId, MessageId, ReservationId, H256};

pub struct Handler<'a, S: Storage> {
    pub program_id: ProgramId,
    pub in_block_transitions: &'a mut InBlockTransitions,
    pub storage: &'a S,
    pub block_info: BlockInfo,
}

impl<S: Storage> Handler<'_, S> {
    pub fn update_state(
        &mut self,
        program_id: ProgramId,
        f: impl FnOnce(&mut ProgramState) -> Result<()>,
    ) -> H256 {
        crate::update_state(self.in_block_transitions, self.storage, program_id, f)
    }

    pub fn update_state_with_storage(
        &mut self,
        program_id: ProgramId,
        f: impl FnOnce(&S, &mut ProgramState) -> Result<()>,
    ) -> H256 {
        crate::update_state_with_storage(self.in_block_transitions, self.storage, program_id, f)
    }

    fn pop_queue_message(state: &ProgramState, storage: &S) -> (H256, MessageId) {
        let mut queue = state
            .queue_hash
            .with_hash_or_default(|hash| storage.read_queue(hash).expect("Failed to read queue"));

        let dispatch = queue
            .pop_front()
            .unwrap_or_else(|| unreachable!("Queue must not be empty in message consume"));

        let new_queue_hash = storage.write_queue(queue);

        (new_queue_hash, dispatch.id)
    }
}

impl<S: Storage> JournalHandler for Handler<'_, S> {
    fn message_dispatched(
        &mut self,
        message_id: MessageId,
        source: ProgramId,
        outcome: DispatchOutcome,
    ) {
        match outcome {
            DispatchOutcome::Exit { program_id } => {
                log::trace!("Dispatch outcome exit: {message_id}")
            }

            DispatchOutcome::InitSuccess { program_id } => {
                log::trace!("Dispatch {message_id} successfully initialized program {program_id}");

                self.update_state(program_id, |state| {
                    match &mut state.program {
                        Program::Active(ActiveProgram { initialized, .. }) if *initialized => {
                            anyhow::bail!("an attempt to initialize already initialized program")
                        }
                        Program::Active(ActiveProgram {
                            ref mut initialized,
                            ..
                        }) => *initialized = true,
                        _ => anyhow::bail!(
                            "an attempt to dispatch init message for inactive program"
                        ),
                    };

                    Ok(())
                });
            }

            DispatchOutcome::InitFailure {
                program_id,
                origin,
                reason,
            } => {
                log::trace!("Dispatch {message_id} failed init of program {program_id}: {reason}");

                self.update_state(program_id, |state| {
                    state.program = Program::Terminated(origin);
                    Ok(())
                });
            }

            DispatchOutcome::MessageTrap { program_id, trap } => {
                log::trace!("Dispatch {message_id} trapped");
                log::debug!("🪤 Program {program_id} terminated with a trap: {trap}");
            }

            DispatchOutcome::Success => log::trace!("Dispatch {message_id} succeed"),

            DispatchOutcome::NoExecution => log::trace!("Dispatch {message_id} wasn't executed"),
        }
    }

    fn gas_burned(&mut self, message_id: MessageId, amount: u64) {
        // TODO
        // unreachable!("Must not be called here")
    }

    fn exit_dispatch(&mut self, id_exited: ProgramId, value_destination: ProgramId) {
        // TODO (breathx): upd contract on exit and send value.
        self.update_state(id_exited, |state| {
            state.program = Program::Exited(value_destination);
            Ok(())
        });
    }

    fn message_consumed(&mut self, message_id: MessageId) {
        self.update_state_with_storage(self.program_id, |storage, state| {
            state.queue_hash = storage.modify_queue(state.queue_hash.clone(), |queue| {
                let queue_head = queue
                    .pop_front()
                    .expect("an attempt to consume message from empty queue");

                assert_eq!(
                    queue_head.id, message_id,
                    "queue head doesn't match processed message"
                );
            })?;

            Ok(())
        });
    }

    fn send_dispatch(
        &mut self,
        message_id: MessageId,
        dispatch: CoreDispatch,
        delay: u32,
        reservation: Option<ReservationId>,
    ) {
        if reservation.is_some() || dispatch.gas_limit().map(|v| v != 0).unwrap_or(false) {
            unreachable!("deprecated: {dispatch:?}");
        }

        if delay != 0 {
            todo!("delayed sending isn't supported yet");
        }

        if self
            .in_block_transitions
            .state_of(&dispatch.destination())
            .is_none()
        {
            if !dispatch.is_reply() {
                let expiry = self.block_info.height + state::MAILBOX_VALIDITY;

                self.update_state_with_storage(dispatch.source(), |storage, state| {
                    state.mailbox_hash =
                        storage.modify_mailbox(state.mailbox_hash.clone(), |mailbox| {
                            mailbox
                                .entry(dispatch.destination())
                                .or_default()
                                .insert(dispatch.id(), (dispatch.value(), expiry));
                        })?;

                    Ok(())
                });
            }

            // TODO (breathx): send here to in_block_transitions.
            let source = dispatch.source();
            let message = dispatch.into_parts().1;

            let source_state_hash = self
                .in_block_transitions
                .state_of(&source)
                .expect("must exist");

            self.in_block_transitions.modify_state_with(
                source,
                source_state_hash,
                0,
                vec![],
                vec![OutgoingMessage::from(message)],
            );

            return;
        }

        let (kind, message) = dispatch.into_parts();
        let (id, source, destination, payload, gas_limit, value, details) = message.into_parts();

        let payload_hash = self.storage.write_payload(payload).into();

        let dispatch = Dispatch {
            id,
            kind,
            source,
            payload_hash,
            value,
            details,
            context: None,
        };

        self.update_state_with_storage(destination, |storage, state| {
            state.queue_hash = storage.modify_queue(state.queue_hash.clone(), |queue| {
                queue.push_back(dispatch);
            })?;
            Ok(())
        });
    }

    fn wait_dispatch(
        &mut self,
        dispatch: StoredDispatch,
        duration: Option<u32>,
        waited_type: MessageWaitedType,
    ) {
        let Some(duration) = duration else {
            todo!("Wait dispatch without specified duration");
        };

        let in_blocks = NonZeroU32::try_from(duration).expect("must be checked on backend side");

        let expiry = self.in_block_transitions.schedule_task(
            in_blocks,
            ScheduledTask::WakeMessage(dispatch.destination(), dispatch.id()),
        );

        let dispatch = Dispatch::from_stored(self.storage, dispatch);

        self.update_state_with_storage(self.program_id, |storage, state| {
            state.queue_hash = storage.modify_queue(state.queue_hash.clone(), |queue| {
                let queue_head = queue
                    .pop_front()
                    .expect("an attempt to wait message from empty queue");

                assert_eq!(
                    queue_head.id, dispatch.id,
                    "queue head doesn't match processed message"
                );
            })?;

            // TODO (breathx): impl Copy for MaybeHash?
            state.waitlist_hash =
                storage.modify_waitlist(state.waitlist_hash.clone(), |waitlist| {
                    let r = waitlist.insert(dispatch.id, (dispatch, expiry));
                    debug_assert!(r.is_none());
                })?;

            Ok(())
        });
    }

    fn wake_message(
        &mut self,
        message_id: MessageId,
        program_id: ProgramId,
        awakening_id: MessageId,
        delay: u32,
    ) {
        if delay != 0 {
            todo!("Delayed wake message");
        }

        log::trace!("Dispatch {message_id} tries to wake {awakening_id}");

        self.update_state_with_storage(program_id, |storage, state| {
            let Some(((dispatch, _expiry), new_waitlist_hash)) = storage
                .modify_waitlist_if_changed(state.waitlist_hash.clone(), |waitlist| {
                    waitlist.remove(&awakening_id)
                })?
            else {
                return Ok(());
            };

            state.waitlist_hash = new_waitlist_hash;
            state.queue_hash = storage.modify_queue(state.queue_hash.clone(), |queue| {
                queue.push_back(dispatch);
            })?;

            Ok(())
        });
    }

    fn update_pages_data(
        &mut self,
        program_id: ProgramId,
        pages_data: BTreeMap<GearPage, PageBuf>,
    ) {
        self.update_state_with_storage(program_id, |storage, state| {
            let Program::Active(ActiveProgram {
                ref mut pages_hash, ..
            }) = state.program
            else {
                anyhow::bail!("an attempt to update pages data of inactive program");
            };

            let new_pages = storage.store_pages(pages_data);

            *pages_hash = storage.modify_memory_pages(pages_hash.clone(), |pages| {
                for (page, data) in new_pages {
                    pages.insert(page, data);
                }
            })?;

            Ok(())
        });
    }

    fn update_allocations(
        &mut self,
        program_id: ProgramId,
        new_allocations: IntervalsTree<WasmPage>,
    ) {
        self.update_state_with_storage(program_id, |storage, state| {
            let Program::Active(ActiveProgram {
                ref mut allocations_hash,
                ..
            }) = state.program
            else {
                anyhow::bail!("an attempt to update allocations of inactive program");
            };

            // TODO (breathx): remove data for difference pages.
            *allocations_hash =
                storage.modify_allocations(allocations_hash.clone(), |allocations| {
                    *allocations = new_allocations;
                })?;

            Ok(())
        });
    }

    fn send_value(&mut self, from: ProgramId, to: Option<ProgramId>, value: u128) {
        // TODO: implement
    }

    fn store_new_programs(
        &mut self,
        _program_id: ProgramId,
        _code_id: CodeId,
        _candidates: Vec<(MessageId, ProgramId)>,
    ) {
        todo!()
    }

    fn stop_processing(&mut self, dispatch: StoredDispatch, gas_burned: u64) {
        todo!()
    }

    fn reserve_gas(&mut self, _: MessageId, _: ReservationId, _: ProgramId, _: u64, _: u32) {
        unreachable!("deprecated");
    }

    fn unreserve_gas(&mut self, _: ReservationId, _: ProgramId, _: u32) {
        unreachable!("deprecated");
    }

    fn update_gas_reservation(&mut self, _: ProgramId, _: GasReserver) {
        unreachable!("deprecated");
    }

    fn system_reserve_gas(&mut self, _: MessageId, _: u64) {
        unreachable!("deprecated");
    }

    fn system_unreserve_gas(&mut self, _: MessageId) {
        unreachable!("deprecated");
    }

    fn send_signal(&mut self, _: MessageId, _: ProgramId, _: SignalCode) {
        unreachable!("deprecated");
    }

    fn reply_deposit(&mut self, _: MessageId, _: MessageId, _: u64) {
        unreachable!("deprecated");
    }
}
