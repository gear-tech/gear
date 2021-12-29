use crate::{
    pallet::Reason, Authorship, Config, DispatchOutcome, Event, ExecutionResult, MessageInfo,
    Pallet, ProgramsLimbo,
};
use codec::Decode;
use common::{
    value_tree::{ConsumeResult, ValueView},
    GasToFeeConverter, Origin, ProgramState, GAS_VALUE_PREFIX, STORAGE_MESSAGE_PREFIX,
    STORAGE_PROGRAM_PREFIX,
};
use core_processor::common::{
    CollectState, Dispatch, DispatchOutcome as CoreDispatchOutcome, JournalHandler, State,
};
use frame_support::{
    storage::PrefixIterator,
    traits::{BalanceStatus, ReservableCurrency},
};
use gear_core::{
    memory::PageNumber,
    message::{Message, MessageId},
    program::{Program, ProgramId},
};
use primitive_types::H256;
use sp_runtime::traits::UniqueSaturatedInto;
use sp_std::{
    collections::{btree_map::BTreeMap, btree_set::BTreeSet, vec_deque::VecDeque},
    marker::PhantomData,
    prelude::*,
};

pub struct ExtManager<T: Config> {
    _phantom: PhantomData<T>,
}

#[derive(Decode)]
struct Node {
    value: Message,
    next: Option<H256>,
}

impl<T> CollectState for ExtManager<T>
where
    T: Config,
    T::AccountId: Origin,
{
    fn collect(&self) -> State {
        let programs: BTreeMap<ProgramId, Program> = PrefixIterator::<H256>::new(
            STORAGE_PROGRAM_PREFIX.to_vec(),
            STORAGE_PROGRAM_PREFIX.to_vec(),
            |key, _| Ok(H256::from_slice(key)),
        )
        .map(|k| {
            let program = self.get_program(k).expect("Can't fail");
            (program.id(), program)
        })
        .collect();

        let mq_head_key = [STORAGE_MESSAGE_PREFIX, b"head"].concat();
        let mut message_queue = VecDeque::new();

        if let Some(head) = sp_io::storage::get(&mq_head_key) {
            let mut next_id = H256::from_slice(&head[..]);
            loop {
                let next_node_key = [STORAGE_MESSAGE_PREFIX, next_id.as_bytes()].concat();
                if let Some(bytes) = sp_io::storage::get(&next_node_key) {
                    let current_node = Node::decode(&mut &bytes[..]).unwrap();
                    message_queue.push_back(current_node.value);
                    match current_node.next {
                        Some(h) => next_id = h,
                        None => break,
                    }
                }
            }
        }

        State {
            message_queue,
            programs,
            ..Default::default()
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
        common::native::get_program(ProgramId::from_origin(id))
    }

    pub(super) fn set_program(&self, program: gear_core::program::Program) {
        let persistent_pages: BTreeMap<u32, Vec<u8>> = program
            .get_pages()
            .iter()
            .map(|(k, v)| (k.raw(), v.to_vec()))
            .collect();

        let id = program.id().into_origin();

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
                let program_id = program.id().into_origin();
                let event = Event::InitSuccess(MessageInfo {
                    message_id: message_id.into_origin(),
                    origin: origin.into_origin(),
                    program_id,
                });

                if common::get_program_state(program_id).is_none() {
                    self.set_program(program);
                } else {
                    Pallet::<T>::wake_waiting_messages(program_id);
                }

                common::set_program_state(program_id, ProgramState::Initialized);

                event
            }
            CoreDispatchOutcome::InitWait {
                message_id,
                program,
                ..
            } => {
                let program_id = program.id().into_origin();
                if common::get_program_state(program_id).is_none() {
                    common::set_program_state(
                        program_id,
                        ProgramState::Uninitialized {
                            message_id: message_id.into_origin(),
                        },
                    );
                    self.set_program(program);
                }

                return;
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

        log::debug!("burned: {:?} from: {:?}", amount, message_id);

        Pallet::<T>::decrease_gas_allowance(amount);
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
                log::debug!("unreserve: {}", gas_left);

                let refund = T::GasConverter::gas_to_fee(gas_left);

                let _ = T::Currency::unreserve(
                    &<T::AccountId as Origin>::from_origin(external),
                    refund,
                );
            } else {
                log::error!(
                    "Associated gas tree for message aren't able to be consumed: {:?}",
                    message_id
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
        let message: common::Message = message.into();

        log::debug!("Message sent (from: {:?}): {:?}", message_id, message);

        if let Some(mut gas_tree) = ValueView::get(GAS_VALUE_PREFIX, message_id) {
            let _ = gas_tree.split_off(message.id, message.gas_limit);
        } else {
            log::error!(
                "Message does not have associated gas tree: {:?}",
                message_id
            );
        }

        if common::program_exists(message.dest) {
            common::queue_message(message);
        } else {
            Pallet::<T>::insert_to_mailbox(message.dest, message.clone());
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
    fn update_nonce_and_pages_amount(
        &mut self,
        program_id: ProgramId,
        persistent_pages: BTreeSet<u32>,
        nonce: u64,
    ) {
        common::set_nonce_and_persistent_pages(program_id.into_origin(), persistent_pages, nonce);
    }
    fn update_page(&mut self, program_id: ProgramId, page_number: PageNumber, data: Vec<u8>) {
        common::set_program_page(program_id.into_origin(), page_number.raw(), data);
    }
}
