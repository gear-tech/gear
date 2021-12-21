use crate::{
    pallet::Reason, Authorship, Config, DispatchOutcome, Event, ExecutionResult, GasAllowance,
    MessageInfo, Pallet, ProgramsLimbo,
};
use common::value_tree::{ConsumeResult, ValueView};
use common::GasToFeeConverter;
use common::Origin;
use common::GAS_VALUE_PREFIX;
use core::marker::PhantomData;
use core_processor::common::{
    CollectState, Dispatch, DispatchOutcome as CoreDispatchOutcome, JournalHandler, State,
};
use frame_support::traits::{BalanceStatus, ReservableCurrency};
use gear_core::{
    memory::PageNumber,
    message::{Message, MessageId},
    program::{Program, ProgramId},
};
use primitive_types::H256;
use sp_runtime::traits::UniqueSaturatedInto;

use sp_std::{
    collections::{btree_map::BTreeMap, vec_deque::VecDeque},
    prelude::*,
};

pub struct ExtManager<T: Config> {
    _phantom: PhantomData<T>,
}

impl<T: Config> CollectState for ExtManager<T> {
    fn collect(&self) -> State {
        State {
            message_queue: VecDeque::new(),
            log: Vec::new(),
            programs: BTreeMap::new(),
            current_failed: false,
        }
    }
}

impl<T> Default for ExtManager<T>
where
    T: Config,
    T::AccountId: Origin,
{
    fn default() -> Self {
        ExtManager {
            _phantom: PhantomData,
        }
    }
}

impl<T> ExtManager<T>
where
    T: Config,
    T::AccountId: Origin,
{
    pub fn new() -> Self {
        Default::default()
    }

    pub fn get_program(&self, id: H256) -> Option<gear_core::program::Program> {
        if let Some(prog) = common::get_program(id) {
            let persistent_pages = common::get_program_pages(id, prog.persistent_pages);
            let code = common::get_code(prog.code_hash)?;
            let id: ProgramId = id.as_ref().into();
            let mut program = Program::new(id, code, persistent_pages).expect("Can't fail");
            program.set_message_nonce(prog.nonce);
            return Some(program);
        };

        None
    }

    pub fn set_program(&self, program: gear_core::program::Program) {
        let mut persistent_pages = BTreeMap::<u32, Vec<u8>>::new();

        for (key, value) in program.get_pages().iter() {
            persistent_pages.insert(key.raw(), value.to_vec());
        }

        let id = H256::from_slice(program.id().as_slice());

        let code_hash: H256 = sp_io::hashing::blake2_256(program.code()).into();

        common::set_code(code_hash, program.code());

        let program = common::Program {
            static_pages: program.static_pages(),
            nonce: program.message_nonce(),
            persistent_pages: persistent_pages.keys().copied().collect(),
            code_hash,
        };

        common::set_program(id, program, persistent_pages);
    }
}

impl<T> JournalHandler for ExtManager<T>
where
    T: Config,
    T::AccountId: Origin,
{
    fn message_dispatched(&mut self, outcome: CoreDispatchOutcome) {
        let event = match outcome {
            CoreDispatchOutcome::Success(message_id) => Event::MessageDispatched(DispatchOutcome {
                message_id: message_id.into_origin(),
                outcome: ExecutionResult::Success,
            }),
            CoreDispatchOutcome::MessageTrap { message_id, trap } => {
                let reason = trap.map(|v| v.as_bytes().to_vec()).unwrap_or_default();

                Event::MessageDispatched(DispatchOutcome {
                    message_id: message_id.into_origin(),
                    outcome: ExecutionResult::Failure(reason),
                })
            }
            CoreDispatchOutcome::InitSuccess {
                message_id,
                origin,
                program,
            } => {
                let event = Event::InitSuccess(MessageInfo {
                    message_id: message_id.into_origin(),
                    origin: origin.into_origin(),
                    program_id: program.id().into_origin(),
                });

                self.set_program(program);

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

                ProgramsLimbo::<T>::insert(program_id, origin);
                log::info!(
                    target: "runtime::gear",
                    "👻 Program {} will stay in limbo until explicitly removed",
                    program_id
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
        };

        Pallet::<T>::deposit_event(event);
    }
    fn gas_burned(&mut self, message_id: MessageId, origin: ProgramId, amount: u64) {
        let message_id = message_id.into_origin();

        // Adjust block gas allowance
        GasAllowance::<T>::mutate(|x| *x = x.saturating_sub(amount));
        // TODO: weight to fee calculator might not be identity fee
        let charge = T::GasConverter::gas_to_fee(amount);

        if let Some(mut gas_tree) = ValueView::get(GAS_VALUE_PREFIX, message_id) {
            gas_tree.spend(amount);
        } else {
            log::error!(
                "Message does not have associated gas tree: {:?}",
                message_id
            );
        }

        let _ = T::Currency::repatriate_reserved(
            &<T::AccountId as Origin>::from_origin(origin.into_origin()),
            &Authorship::<T>::author(),
            charge,
            BalanceStatus::Free,
        );
    }
    fn message_consumed(&mut self, message_id: MessageId) {
        let message_id = message_id.into_origin();

        if let Some(gas_tree) = ValueView::get(GAS_VALUE_PREFIX, message_id) {
            if let ConsumeResult::RefundExternal(external, gas_left) = gas_tree.consume() {
                let refund = T::GasConverter::gas_to_fee(gas_left);

                let _ = T::Currency::unreserve(
                    &<T::AccountId as Origin>::from_origin(external),
                    refund,
                );
            }
        } else {
            log::error!(
                "Message does not have associated gas tree: {:?}",
                message_id
            );
        }
    }
    fn send_message(&mut self, message_id: MessageId, message: Message) {
        let message_id = message_id.into_origin();
        let dest = message.dest().into_origin();
        let message: common::Message = message.into();

        if common::program_exists(dest) {
            if let Some(mut gas_tree) = ValueView::get(GAS_VALUE_PREFIX, message_id) {
                let _ = gas_tree.split_off(message.id, message.gas_limit);
            } else {
                log::error!(
                    "Message does not have associated gas tree: {:?}",
                    message_id
                );
            }

            common::queue_message(message);
        } else {
            Pallet::<T>::insert_to_mailbox(dest, message.clone());
            Pallet::<T>::deposit_event(Event::Log(message));
        }
    }
    fn wait_dispatch(&mut self, dispatch: Dispatch) {
        let message: common::Message = dispatch.message.into();

        common::insert_waiting_message(
            message.dest,
            message.id,
            message.clone(),
            <frame_system::Pallet<T>>::block_number().unique_saturated_into(),
        );

        Pallet::<T>::deposit_event(Event::AddedToWaitList(message));
    }
    fn wake_message(
        &mut self,
        message_id: MessageId,
        program_id: ProgramId,
        awakening_id: MessageId,
    ) {
        let awakening_id = awakening_id.into_origin();

        if let Some((msg, _)) =
            common::remove_waiting_message(program_id.into_origin(), awakening_id)
        {
            common::queue_message(msg);

            Pallet::<T>::deposit_event(Event::RemovedFromWaitList(awakening_id));
        } else {
            log::error!(
                "Unknown message awaken: {:?} from {:?}",
                awakening_id,
                message_id.into_origin()
            );
        }
    }
    fn update_nonce(&mut self, program_id: ProgramId, nonce: u64) {
        common::set_nonce(program_id.into_origin(), nonce);
    }
    fn update_page(&mut self, program_id: ProgramId, page_number: PageNumber, data: Vec<u8>) {
        common::set_program_page(program_id.into_origin(), page_number.raw(), data);
    }
}
