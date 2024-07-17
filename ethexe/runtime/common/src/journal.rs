use crate::state::{self, HashAndLen, MaybeHash, ProgramState, Storage};
use alloc::{collections::BTreeMap, vec::Vec};
use core_processor::{
    common::{DispatchOutcome, JournalHandler},
    configs::BlockInfo,
};
use gear_core::{
    ids::ProgramId,
    memory::PageBuf,
    message::{Dispatch, MessageWaitedType, Payload, StoredDispatch, StoredMessage},
    pages::{numerated::tree::IntervalsTree, GearPage, WasmPage},
    program::ProgramState as InitStatus,
    reservation::GasReserver,
};
use gear_core_errors::SignalCode;
use gprimitives::{ActorId, CodeId, MessageId, ReservationId, H256};

pub struct Handler<'a, S: Storage> {
    pub program_id: ProgramId,
    pub program_states: &'a mut BTreeMap<ProgramId, H256>,
    pub storage: &'a S,
    pub block_info: BlockInfo,
    // TODO: replace with something reasonable.
    pub results: BTreeMap<ActorId, (H256, H256)>,
    pub to_users_messages: Vec<StoredMessage>,
}

impl<S: Storage> Handler<'_, S> {
    #[track_caller]
    pub fn update_program(
        &mut self,
        program_id: ProgramId,
        f: impl FnOnce(ProgramState, &S) -> Option<ProgramState>,
    ) {
        let state_hash = self
            .program_states
            .get_mut(&program_id)
            .expect("Program does not exist");
        let program_state = self
            .storage
            .read_state(*state_hash)
            .expect("Failed to read state");

        let initial_state = *state_hash;

        if let Some(program_new_state) = f(program_state, self.storage) {
            *state_hash = self.storage.write_state(program_new_state);

            self.results
                .entry(program_id)
                .and_modify(|v| v.1 = *state_hash)
                .or_insert_with(|| (initial_state, *state_hash));
        }
    }

    fn pop_queue_message(state: &ProgramState, storage: &S) -> (H256, MessageId) {
        let mut queue = state
            .queue_hash
            .with_hash_or_default(|hash| storage.read_queue(hash).expect("Failed to read queue"));

        let dispatch = queue
            .pop_front()
            .unwrap_or_else(|| unreachable!("Queue must not be empty in message consume"));

        let new_queue_hash = storage.write_queue(queue);

        (new_queue_hash, dispatch.id())
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
            DispatchOutcome::Exit { .. } => todo!(),
            DispatchOutcome::InitSuccess { program_id } => {
                log::trace!("Dispatch {message_id} init success for program {program_id}");
                self.update_program(program_id, |mut state, _| match &mut state.state {
                    state::Program::Active(program) => {
                        program.initialized = true;
                        Some(state)
                    }
                    _ => None,
                });
            }
            DispatchOutcome::InitFailure {
                program_id,
                origin,
                reason,
            } => {
                log::trace!(
                    "Init failed for dispatch {message_id}, program {program_id}: {reason}"
                );
                self.update_program(program_id, |state, _| {
                    Some(ProgramState {
                        state: state::Program::Terminated(origin),
                        ..state
                    })
                });
                // TODO: return gas reservations
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
        // TODO
        // unreachable!("Must not be called here")
    }

    fn exit_dispatch(&mut self, id_exited: ProgramId, value_destination: ProgramId) {
        self.update_program(id_exited, |state, _| {
            Some(ProgramState {
                state: state::Program::Exited(value_destination),
                ..state
            })
        });
        // TODO: return gas reservations
    }

    fn message_consumed(&mut self, message_id: MessageId) {
        self.update_program(self.program_id, |state, storage| {
            let (queue_hash, pop_id) = Self::pop_queue_message(&state, storage);

            if pop_id != message_id {
                unreachable!("First message in queue is {pop_id}, but {message_id} was consumed",);
            }

            Some(ProgramState {
                queue_hash: queue_hash.into(),
                ..state
            })
        });

        // TODO: implement returning of system reservation and left gas
    }

    fn send_dispatch(
        &mut self,
        message_id: MessageId,
        dispatch: Dispatch,
        delay: u32,
        reservation: Option<ReservationId>,
    ) {
        let dispatch = dispatch.into_stored();

        if reservation.is_some() {
            unreachable!("deprecated");
        }

        if delay != 0 {
            todo!("delayed sending isn't supported yet")
        }

        if !self.program_states.contains_key(&dispatch.destination()) {
            self.to_users_messages.push(dispatch.into_parts().1);
            return;
        };

        let dispatch = dispatch.cast(|payload| self.storage.write_payload(payload).into());

        self.update_program(dispatch.destination(), |state, storage| {
            let mut queue = state.queue_hash.with_hash_or_default(|hash| {
                storage.read_queue(hash).expect("Failed to read queue")
            });

            queue.push_back(dispatch);

            let queue_hash = storage.write_queue(queue).into();
            Some(ProgramState {
                queue_hash,
                ..state
            })
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

        let dispatch = dispatch.cast(|payload| self.storage.write_payload(payload).into());

        let block = self.block_info.height.saturating_add(duration);

        log::trace!("Adding {:?} to waitlist (#{block})", dispatch);

        self.update_program(dispatch.destination(), |state, storage| {
            let (queue_hash, pop_id) = Self::pop_queue_message(&state, storage);

            if pop_id != dispatch.id() {
                // TODO (breathx): figure out what's it.
                unreachable!(
                    "First message in queue is {pop_id}, but {} was waited",
                    dispatch.id()
                );
            }

            let mut waitlist = state.waitlist_hash.with_hash_or_default(|hash| {
                storage
                    .read_waitlist(hash)
                    .expect("Failed to read waitlist")
            });

            waitlist.entry(block).or_default().push(dispatch);

            Some(ProgramState {
                waitlist_hash: storage.write_waitlist(waitlist).into(),
                queue_hash: queue_hash.into(),
                ..state
            })
        });
    }

    fn wake_message(
        &mut self,
        message_id: MessageId,
        program_id: ProgramId,
        awakening_id: MessageId,
        delay: u32,
    ) {
        log::trace!("Message {message_id} try to wake {awakening_id}");

        if delay != 0 {
            todo!("Delayed wake message");
        }

        self.update_program(program_id, |state, storage| {
            let mut waitlist = state.waitlist_hash.with_hash_or_default(|hash| {
                storage
                    .read_waitlist(hash)
                    .expect("Failed to read waitlist")
            });

            let mut queue = state.queue_hash.with_hash_or_default(|hash| {
                storage.read_queue(hash).expect("Failed to read queue")
            });

            let mut changed = false;
            let mut clear_for_block = None;
            for (block, list) in waitlist.iter_mut() {
                let Some(index) = list
                    .iter()
                    .enumerate()
                    .find_map(|(index, dispatch)| (dispatch.id() == awakening_id).then_some(index))
                else {
                    continue;
                };

                let dispatch = list.remove(index);
                log::trace!("{dispatch:?} has been woken up by {message_id}");

                queue.push_back(dispatch);

                if list.is_empty() {
                    clear_for_block = Some(*block);
                }
                changed = true;
                break;
            }

            if let Some(block) = clear_for_block {
                waitlist.remove(&block);
            }

            changed.then(|| {
                let queue_hash = storage.write_queue(queue).into();
                let waitlist_hash = storage.write_waitlist(waitlist).into();
                ProgramState {
                    queue_hash,
                    waitlist_hash,
                    ..state
                }
            })
        });
    }

    fn update_pages_data(
        &mut self,
        program_id: ProgramId,
        pages_data: BTreeMap<GearPage, PageBuf>,
    ) {
        let mut new_page_hashes = BTreeMap::new();
        for (page, data) in pages_data {
            let hash = self.storage.write_page_data(data);
            new_page_hashes.insert(page, hash);
        }

        self.update_program(program_id, |state, storage| {
            let state::Program::Active(mut active_state) = state.state else {
                return None;
            };

            let mut pages_map = active_state.pages_hash.with_hash_or_default(|hash| {
                storage.read_pages(hash).expect("Failed to read pages")
            });

            for (page, hash) in new_page_hashes {
                pages_map.insert(page, hash);
            }

            let changed_active_state = state::ActiveProgram {
                pages_hash: storage.write_pages(pages_map).into(),
                ..active_state
            };

            Some(ProgramState {
                state: state::Program::Active(changed_active_state),
                ..state
            })
        });
    }

    fn update_allocations(&mut self, program_id: ProgramId, allocations: IntervalsTree<WasmPage>) {
        todo!()
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
