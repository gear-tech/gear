use gear_common::{Origin, ProgramState};
use gear_core::{
    memory::PageNumber,
    message::{Message, MessageId},
    program::ProgramId,
};
use gear_core_processor::common::{
    CollectState, Dispatch, DispatchKind, DispatchOutcome as CoreDispatchOutcome, JournalHandler,
    State,
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

    fn message_to_dispatch(&self, message: Message) -> Dispatch {
        let kind = if message.reply.is_some() {
            DispatchKind::HandleReply
        } else {
            match gear_common::get_program_state(message.dest().into_origin())
                .expect("Program should be in the storage")
            {
                ProgramState::Initialized => DispatchKind::Handle,
                ProgramState::Uninitialized { message_id } => {
                    assert_eq!(message_id, message.id().into_origin());
                    DispatchKind::Init
                }
            }
        };

        Dispatch { kind, message }
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
            CoreDispatchOutcome::InitFailure { .. } => {
                self.current_failed = true;
            }
            CoreDispatchOutcome::InitSuccess { message_id, .. } => {
                self.current_failed = false;

                if let Some(next_message) = gear_common::message_iter().next() {
                    if next_message.id == message_id.into_origin() {
                        gear_common::dequeue_message();
                    }
                }
            }
            CoreDispatchOutcome::Success(_) => {
                self.current_failed = false;
                let _ = gear_common::dequeue_message();
            }
            CoreDispatchOutcome::MessageTrap { .. } => {
                self.current_failed = true;
                let _ = gear_common::dequeue_message();
            }
        }
        self.inner.message_dispatched(outcome)
    }

    fn exit_dispatch(&mut self, _id_exited: ProgramId, _value_destination: ProgramId) {
        unimplemented!()
    }

    fn gas_burned(&mut self, message_id: MessageId, origin: ProgramId, amount: u64) {
        self.inner.gas_burned(message_id, origin, amount)
    }
    fn message_consumed(&mut self, message_id: MessageId) {
        self.inner.message_consumed(message_id)
    }
    fn send_message(&mut self, message_id: MessageId, message: Message) {
        let program_id = message.dest().into_origin();
        match gear_common::get_program_state(program_id) {
            None => self.log.push(message.clone()),
            Some(state) => {
                if let (None, ProgramState::Uninitialized { message_id }) = (message.reply(), state)
                {
                    assert_ne!(message_id, message.id().into_origin());

                    let message_id = message.id().into_origin();
                    gear_common::waiting_init_append_message_id(program_id, message_id);
                    gear_common::insert_waiting_message(
                        program_id,
                        message_id,
                        message.into(),
                        // TODO: retrieve block number
                        0,
                    );
                    return;
                }
            }
        }

        self.inner.send_message(message_id, message);
    }
    fn wait_dispatch(&mut self, dispatch: Dispatch) {
        self.current_failed = false;

        if let Some(next_message) = gear_common::message_iter().next() {
            if next_message.id == dispatch.message.id().into_origin() {
                gear_common::dequeue_message();
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
}
