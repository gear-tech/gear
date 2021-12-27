use gear_common::Origin;
use gear_core::{
    memory::PageNumber,
    message::{Message, MessageId},
    program::ProgramId,
};
use gear_core_processor::common::{
    CollectState, Dispatch, DispatchOutcome as CoreDispatchOutcome, JournalHandler, State,
};
use gear_runtime::{pallet_gear::Config, ExtManager};
use std::collections::BTreeSet;

pub struct RuntestsExtManager<T: Config> {
    log: Vec<Message>,
    inner: ExtManager<T>,
    current_failed: bool,
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
            inner: ExtManager::<T>::new(),
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
            CoreDispatchOutcome::InitSuccess { .. } => {
                self.current_failed = false;
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
    fn gas_burned(&mut self, message_id: MessageId, origin: ProgramId, amount: u64) {
        self.inner.gas_burned(message_id, origin, amount)
    }
    fn message_consumed(&mut self, message_id: MessageId) {
        self.inner.message_consumed(message_id)
    }
    fn send_message(&mut self, message_id: MessageId, message: Message) {
        if !gear_common::program_exists(message.dest().into_origin()) {
            self.log.push(message.clone())
        };

        self.inner.send_message(message_id, message);
    }
    fn wait_dispatch(&mut self, dispatch: Dispatch) {
        self.current_failed = false;
        let _ = gear_common::dequeue_message();
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
    fn update_nonce_and_pages_amount(
        &mut self,
        program_id: ProgramId,
        persistent_pages: BTreeSet<u32>,
        nonce: u64,
    ) {
        self.inner
            .update_nonce_and_pages_amount(program_id, persistent_pages, nonce)
    }
    fn update_page(&mut self, program_id: ProgramId, page_number: PageNumber, data: Vec<u8>) {
        self.inner.update_page(program_id, page_number, data)
    }
}
