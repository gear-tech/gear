use crate::{
    pallet::Reason, Authorship, Config, DispatchOutcome, Event, ExecutionResult, GasAllowance,
    MessageInfo, Pallet, ProgramsLimbo,
};
use common::GasToFeeConverter;
use common::Origin;
use common::GAS_VALUE_PREFIX;
use core::marker::PhantomData;
use core_processor::common::{CollectState, Dispatch, DispatchKind, JournalHandler, State};
use frame_support::traits::{BalanceStatus, Currency, ExistenceRequirement, ReservableCurrency};
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

    pub fn get_program(&self, id: H256) -> Result<gear_core::program::Program, u8> {
        if let Some(prog) = common::get_program(id) {
            let persistent_pages = common::get_program_pages(id, prog.persistent_pages);
            let code = common::get_code(prog.code_hash).ok_or(1)?;
            let id: gear_core::program::ProgramId = id.as_ref().into();
            let mut program =
                gear_core::program::Program::new(id, code, persistent_pages).expect("Can't fail");
            program.set_message_nonce(prog.nonce);
            return Ok(program);
        };

        Err(1)
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
    fn execution_fail(
        &mut self,
        origin: MessageId,
        initiator: ProgramId,
        program_id: ProgramId,
        reason: &'static str,
        entry: DispatchKind,
    ) {
        let origin = H256::from_slice(origin.as_slice());
        let program_id = H256::from_slice(program_id.as_slice());

        if let DispatchKind::Init = entry {
            ProgramsLimbo::<T>::insert(program_id, origin);
            log::info!(
                target: "runtime::gear",
                "👻 Program {} will stay in limbo until explicitly removed",
                program_id
            );

            Pallet::<T>::deposit_event(Event::InitFailure(
                MessageInfo {
                    message_id: origin,
                    program_id,
                    origin: H256::from_slice(initiator.as_slice()),
                },
                Reason::Dispatch(reason.as_bytes().to_vec()),
            ));
        } else {
            match common::value_tree::ValueView::get(GAS_VALUE_PREFIX, origin) {
                Some(gas_tree) => {
                    if let common::value_tree::ConsumeResult::RefundExternal(external, gas_left) =
                        gas_tree.consume()
                    {
                        let refund = T::GasConverter::gas_to_fee(gas_left);

                        let _ = T::Currency::unreserve(
                            &<T::AccountId as Origin>::from_origin(external),
                            refund,
                        );
                    }
                }
                None => {
                    log::error!("Message does not have associated gas tree: {:?}", origin);
                }
            }

            Pallet::<T>::deposit_event(Event::MessageDispatched(DispatchOutcome {
                message_id: origin,
                outcome: ExecutionResult::Failure(reason.as_bytes().to_vec()),
            }));
        }
    }
    fn gas_burned(&mut self, origin: MessageId, amount: u64, entry: DispatchKind) {
        // Adjust block gas allowance
        GasAllowance::<T>::mutate(|x| *x = x.saturating_sub(amount));

        // TODO: weight to fee calculator might not be identity fee
        let charge = T::GasConverter::gas_to_fee(amount);

        let origin = match common::value_tree::ValueView::get(
            GAS_VALUE_PREFIX,
            H256::from_slice(origin.as_slice()),
        ) {
            Some(mut gas_tree) => {
                gas_tree.spend(amount);
                gas_tree.origin()
            }
            None => {
                log::error!("Message does not have associated gas tree: {:?}", origin);
                Default::default()
            }
        };

        if let DispatchKind::Init = entry {
            if let Err(e) = T::Currency::transfer(
                &<T::AccountId as Origin>::from_origin(origin),
                &Authorship::<T>::author(),
                charge,
                ExistenceRequirement::AllowDeath,
            ) {
                // should not be possible since there should've been reserved enough for
                // the transfer
                // TODO: audit this
                log::warn!("Could not transfer enough gas to block producer: {:?}", e);
            }
        } else {
            let _ = T::Currency::repatriate_reserved(
                &<T::AccountId as Origin>::from_origin(origin),
                &Authorship::<T>::author(),
                charge,
                BalanceStatus::Free,
            );
        }
    }

    fn message_consumed(&mut self, message_id: MessageId) {
        match common::value_tree::ValueView::get(
            GAS_VALUE_PREFIX,
            H256::from_slice(message_id.as_slice()),
        ) {
            Some(gas_tree) => {
                if let common::value_tree::ConsumeResult::RefundExternal(external, gas_left) =
                    gas_tree.consume()
                {
                    let refund = T::GasConverter::gas_to_fee(gas_left);

                    let _ = T::Currency::unreserve(
                        &<T::AccountId as Origin>::from_origin(external),
                        refund,
                    );
                }
            }
            None => {
                log::error!(
                    "Message does not have associated gas tree: {:?}",
                    message_id
                );
            }
        }

        Pallet::<T>::deposit_event(Event::MessageDispatched(DispatchOutcome {
            message_id: H256::from_slice(message_id.as_slice()),
            outcome: ExecutionResult::Success,
        }));
    }

    fn message_trap(&mut self, _origin: MessageId, _trap: Option<&'static str>) {}

    fn send_message(&mut self, origin: MessageId, message: Message) {
        let dest = H256::from_slice(message.dest().as_slice());
        let message: common::Message = message.into();

        if common::program_exists(dest) {
            match common::value_tree::ValueView::get(
                GAS_VALUE_PREFIX,
                H256::from_slice(origin.as_slice()),
            ) {
                Some(mut gas_tree) => {
                    gas_tree.split_off(message.id, message.gas_limit);
                }
                None => {
                    log::error!("Message does not have associated gas tree: {:?}", origin);
                }
            }
            common::queue_message(message);
        } else {
            Pallet::<T>::insert_to_mailbox(dest, message.clone());
            Pallet::<T>::deposit_event(Event::Log(message));
        }
    }
    fn submit_program(&mut self, origin: MessageId, owner: ProgramId, program: Program) {
        Pallet::<T>::deposit_event(Event::InitSuccess(MessageInfo {
            message_id: H256::from_slice(origin.as_slice()),
            program_id: H256::from_slice(program.id().as_slice()),
            origin: H256::from_slice(owner.as_slice()),
        }));
        self.set_program(program);
    }
    fn wait_dispatch(&mut self, dispatch: Dispatch) {
        let message: common::Message = dispatch.message.into();
        Pallet::<T>::deposit_event(Event::AddedToWaitList(message.clone()));
        common::insert_waiting_message(
            message.dest,
            message.id,
            message,
            <frame_system::Pallet<T>>::block_number().unique_saturated_into(),
        );
    }
    fn wake_message(&mut self, _origin: MessageId, program_id: ProgramId, message_id: MessageId) {
        let msg_id = H256::from_slice(message_id.as_slice());
        if let Some((msg, _)) =
            common::remove_waiting_message(H256::from_slice(program_id.as_slice()), msg_id)
        {
            common::queue_message(msg);
            Pallet::<T>::deposit_event(Event::RemovedFromWaitList(msg_id));
        } else {
            log::warn!("Unknown message awaken: {}", msg_id);
        }
    }
    fn update_nonce(&mut self, program_id: ProgramId, nonce: u64) {
        common::set_nonce(H256::from_slice(program_id.as_slice()), nonce);
    }
    fn update_page(&mut self, program_id: ProgramId, page_number: PageNumber, data: Vec<u8>) {
        common::set_program_page(
            H256::from_slice(program_id.as_slice()),
            page_number.raw(),
            data,
        );
    }
}
