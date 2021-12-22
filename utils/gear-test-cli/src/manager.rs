use gear_runtime::pallet_gear::Config;
use gear_runtime::ExtManager;

use gear_common::Origin;
use gear_core::{
    memory::PageNumber,
    message::{Message, MessageId},
    program::ProgramId,
};
use gear_core_processor::common::{
    CollectState, Dispatch, DispatchOutcome as CoreDispatchOutcome, JournalHandler, State,
};

pub struct RuntestsExtManager<T: Config> {
    log: Vec<Message>,
    inner: ExtManager<T>,
}

impl<T> CollectState for RuntestsExtManager<T>
where
    T: Config,
    T::AccountId: Origin,
{
    fn collect(&self) -> State {
        let mut state = self.inner.collect();
        state.log = self.log.clone();

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
            CoreDispatchOutcome::InitFailure { .. } => {}
            CoreDispatchOutcome::InitSuccess { .. } => {}
            _ => {
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
        self.inner.wait_dispatch(dispatch)
    }
    fn wake_message(
        &mut self,
        message_id: MessageId,
        program_id: ProgramId,
        awakening_id: MessageId,
    ) {
        if let Some((msg, _)) = gear_common::remove_waiting_message(
            program_id.into_origin(),
            awakening_id.into_origin(),
        ) {
            gear_common::queue_message(msg);
        } else {
            panic!(
                "Unknown message awaken: {:?} from {:?}",
                awakening_id,
                message_id.into_origin()
            );
        }
        self.inner
            .wake_message(message_id, program_id, awakening_id)
    }
    fn update_nonce(&mut self, program_id: ProgramId, nonce: u64) {
        self.inner.update_nonce(program_id, nonce)
    }
    fn update_page(&mut self, program_id: ProgramId, page_number: PageNumber, data: Vec<u8>) {
        self.inner.update_page(program_id, page_number, data)
    }
}
