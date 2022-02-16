use gear_common::Origin;
use gear_core::{
    memory::PageNumber,
    message::{Dispatch, DispatchKind, Message, MessageId},
    program::ProgramId,
};
use gear_core_processor::common::{
    CollectState, DispatchOutcome as CoreDispatchOutcome, JournalHandler, State,
};
use gear_runtime::{pallet_gear::Config, ExtManager};
use gear_test::check::ExecutionContext;

pub struct RuntestsExtManager<T: Config> {
    log: Vec<Message>,
    inner: ExtManager<T, ()>,
    current_failed: bool,
}

impl<T> ExecutionContext for RuntestsExtManager<T>
where
    T: Config,
    T::AccountId: Origin,
{
    fn store_program(&self, program: gear_core::program::Program, init_message_id: MessageId) {
        self.inner
            .set_program(program, init_message_id.into_origin());
    }
}

impl<T> CollectState for RuntestsExtManager<T>
where
    T: Config,
    T::AccountId: Origin,
{
    fn collect(&self) -> State {
        let mut state = self.inner.collect();
        state.log = self.log.clone();
        state.current_failed = self.current_failed;

        log::debug!("{:?}", state);

        state
    }
}

impl<T> Default for RuntestsExtManager<T>
where
    T: Config,
    T::AccountId: Origin,
{
    fn default() -> Self {
        Self {
            log: Default::default(),
            inner: ExtManager::<T, ()>::default(),
            current_failed: false,
        }
    }
}

impl<T> JournalHandler for RuntestsExtManager<T>
where
    T: Config,
    T::AccountId: Origin,
{
    fn message_dispatched(&mut self, outcome: CoreDispatchOutcome) {
        match outcome {
            CoreDispatchOutcome::InitFailure { message_id, .. } => {
                self.current_failed = true;

                if let Some(dispatch) = gear_common::dispatch_iter().next() {
                    if dispatch.message.id == message_id.into_origin() {
                        gear_common::dequeue_dispatch();
                    }
                }
            }
            CoreDispatchOutcome::InitSuccess { message_id, .. } => {
                self.current_failed = false;

                if let Some(dispatch) = gear_common::dispatch_iter().next() {
                    if dispatch.message.id == message_id.into_origin() {
                        gear_common::dequeue_dispatch();
                    }
                }
            }
            CoreDispatchOutcome::Success(_) | CoreDispatchOutcome::Skip(_) => {
                self.current_failed = false;
                let _ = gear_common::dequeue_dispatch();
            }
            CoreDispatchOutcome::MessageTrap { .. } => {
                self.current_failed = true;
                let _ = gear_common::dequeue_dispatch();
            }
        }
        self.inner.message_dispatched(outcome)
    }
    fn gas_burned(&mut self, message_id: MessageId, origin: ProgramId, amount: u64) {
        self.inner.gas_burned(message_id, origin, amount)
    }

    fn exit_dispatch(&mut self, id_exited: ProgramId, value_destination: ProgramId) {
        self.inner.exit_dispatch(id_exited, value_destination);
    }

    fn message_consumed(&mut self, message_id: MessageId) {
        self.inner.message_consumed(message_id)
    }
    fn send_dispatch(&mut self, message_id: MessageId, dispatch: Dispatch) {
        let program_id = dispatch.message.dest().into_origin();

        match gear_common::get_program(program_id) {
            Some(prog) => {
                if matches!(dispatch.kind, DispatchKind::Handle) && prog.is_uninitialized() {
                    let message_id = message_id.into_origin();
                    gear_common::waiting_init_append_message_id(program_id, message_id);
                    gear_common::insert_waiting_message(program_id, message_id, dispatch.into(), 0);

                    return;
                }
            }
            None => self.log.push(dispatch.message.clone()),
        };

        self.inner.send_dispatch(message_id, dispatch);
    }
    fn wait_dispatch(&mut self, dispatch: Dispatch) {
        self.current_failed = false;

        let current_message_id = dispatch.message.id().into_origin();
        if let Some(next_dispatch) = gear_common::dispatch_iter().next() {
            if next_dispatch.message.id == current_message_id {
                gear_common::dequeue_dispatch();
            }
        }

        self.inner.wait_dispatch(dispatch)
    }
    fn wake_message(
        &mut self,
        message_id: MessageId,
        program_id: ProgramId,
        awakening_id: MessageId,
    ) {
        self.inner
            .wake_message(message_id, program_id, awakening_id)
    }
    fn update_nonce(&mut self, program_id: ProgramId, nonce: u64) {
        self.inner.update_nonce(program_id, nonce)
    }
    fn update_page(
        &mut self,
        program_id: ProgramId,
        page_number: PageNumber,
        data: Option<Vec<u8>>,
    ) {
        self.inner.update_page(program_id, page_number, data)
    }
    fn send_value(&mut self, from: ProgramId, to: Option<ProgramId>, value: u128) {
        self.inner.send_value(from, to, value);
    }
}
