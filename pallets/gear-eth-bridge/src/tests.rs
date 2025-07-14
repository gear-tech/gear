use crate::{
    internal::EthMessageExt,
    mock::{mock_builtin_id as builtin_id, *},
    Config, EthMessage, WeightInfo,
};
use common::Origin as _;
use frame_support::{
    assert_noop, assert_ok, assert_storage_noop, traits::Get, Blake2_256, StorageHasher,
};
use gbuiltin_eth_bridge::{Request, Response};
use gear_core_errors::{ErrorReplyReason, ReplyCode, SimpleExecutionError, SuccessReplyReason};
use pallet_gear::Event as GearEvent;
use pallet_gear_builtin::BuiltinActorError;
use pallet_grandpa::Event as GrandpaEvent;
use pallet_session::Event as SessionEvent;
use parity_scale_codec::{Decode, Encode};
use sp_core::{H160, H256};
use sp_runtime::traits::{BadOrigin, Keccak256};
use utils::*;

const EPOCH_BLOCKS: u64 = EpochDuration::get();
const ERA_BLOCKS: u64 = EPOCH_BLOCKS * SessionsPerEra::get() as u64;
const WHEN_INITIALIZED: u64 = 42;

type AuthoritySetHash = crate::AuthoritySetHash<Test>;
type MessageNonce = crate::MessageNonce<Test>;
type Queue = crate::Queue<Test>;
type QueueChanged = crate::QueueChanged<Test>;
type QueueMerkleRoot = crate::QueueMerkleRoot<Test>;
type Initialized = crate::Initialized<Test>;
type Paused = crate::Paused<Test>;
type Event = crate::Event<Test>;
type Error = crate::Error<Test>;
type Currency = crate::CurrencyOf<Test>;

#[test]
fn bridge_got_initialized() {
    init_logger();
    new_test_ext().execute_with(|| {
        run_to_block(1);
        do_events_assertion(0, 1, []);
        assert!(!Initialized::get());
        assert!(!QueueMerkleRoot::exists());
        assert!(Paused::get());

        run_to_block(EPOCH_BLOCKS);
        do_events_assertion(0, 6, []);

        run_to_block(EPOCH_BLOCKS + 1);
        do_events_assertion(1, 7, [SessionEvent::NewSession { session_index: 1 }.into()]);

        run_to_block(EPOCH_BLOCKS * 2);
        do_events_assertion(1, 12, []);

        run_to_block(EPOCH_BLOCKS * 2 + 1);
        do_events_assertion(
            2,
            13,
            [SessionEvent::NewSession { session_index: 2 }.into()],
        );

        run_to_block(EPOCH_BLOCKS * 3);
        do_events_assertion(2, 18, []);

        run_to_block(EPOCH_BLOCKS * 3 + 1);
        do_events_assertion(
            3,
            19,
            [SessionEvent::NewSession { session_index: 3 }.into()],
        );

        run_to_block(EPOCH_BLOCKS * 4);
        do_events_assertion(3, 24, []);

        run_to_block(EPOCH_BLOCKS * 4 + 1);
        do_events_assertion(
            4,
            25,
            [SessionEvent::NewSession { session_index: 4 }.into()],
        );

        run_to_block(EPOCH_BLOCKS * 5);
        do_events_assertion(4, 30, []);

        run_to_block(EPOCH_BLOCKS * 5 + 1);
        do_events_assertion(
            5,
            31,
            [SessionEvent::NewSession { session_index: 5 }.into()],
        );

        run_to_block(ERA_BLOCKS);
        do_events_assertion(5, 36, []);

        run_to_block(ERA_BLOCKS + 1);
        do_events_assertion(
            6,
            37,
            [
                SessionEvent::NewSession { session_index: 6 }.into(),
                Event::BridgeInitialized.into(),
            ],
        );
        assert_eq!(QueueMerkleRoot::get(), Some(H256::zero()));
        assert!(Initialized::get());
        assert!(Paused::get());

        on_finalize_gear_block(ERA_BLOCKS + 1);
        do_events_assertion(
            6,
            37,
            [GrandpaEvent::NewAuthorities {
                authority_set: era_validators_authority_set(6),
            }
            .into()],
        );
    })
}

#[test]
fn bridge_unpause_works() {
    init_logger();
    new_test_ext().execute_with(|| {
        run_to_block(WHEN_INITIALIZED);

        assert_noop!(
            GearEthBridge::unpause(RuntimeOrigin::signed(SIGNER)),
            BadOrigin
        );

        assert_ok!(GearEthBridge::unpause(RuntimeOrigin::root()));

        System::assert_last_event(Event::BridgeUnpaused.into());

        assert!(!Paused::get());

        assert_storage_noop!(assert_ok!(GearEthBridge::unpause(RuntimeOrigin::root())));
    })
}

#[test]
fn bridge_pause_works() {
    init_logger();
    new_test_ext().execute_with(|| {
        run_to_block(WHEN_INITIALIZED);

        assert_noop!(
            GearEthBridge::pause(RuntimeOrigin::signed(SIGNER)),
            BadOrigin
        );

        assert_storage_noop!(assert_ok!(GearEthBridge::pause(RuntimeOrigin::root())));

        assert_ok!(GearEthBridge::unpause(RuntimeOrigin::root()));

        assert_ok!(GearEthBridge::pause(RuntimeOrigin::root()));

        System::assert_last_event(Event::BridgePaused.into());

        assert!(Paused::get());

        assert_storage_noop!(assert_ok!(GearEthBridge::pause(RuntimeOrigin::root())));
    })
}

#[test]
fn bridge_send_eth_message_works() {
    init_logger();
    new_test_ext().execute_with(|| {
        run_to_block(WHEN_INITIALIZED);

        assert_ok!(GearEthBridge::set_fee(
            RuntimeOrigin::root(),
            MockTransportFee::get()
        ));
        assert_ok!(GearEthBridge::unpause(RuntimeOrigin::root()));

        assert_noop!(
            GearEthBridge::send_eth_message(RuntimeOrigin::root(), H160::zero(), vec![]),
            BadOrigin
        );

        assert_eq!(MessageNonce::get(), 0.into());
        assert!(Queue::get().is_empty());

        // Send a message via the pallet extrinsic

        let destination = H160::random();
        let payload = H256::random().as_bytes().to_vec();
        let mut gas_meter = GasSpentMeter::start();
        let mut signer_balance = balance_of(&SIGNER);
        let builtin_id = AccountId::from_origin(builtin_id().into());
        let mut builtin_balance = balance_of(&builtin_id);
        let fee = MockTransportFee::get();

        let message = unsafe {
            EthMessage::new_unchecked(0.into(), SIGNER.cast(), destination, payload.clone())
        };
        let hash = message.hash();
        let mut queue = vec![hash];

        assert_ok!(GearEthBridge::send_eth_message(
            RuntimeOrigin::signed(SIGNER),
            destination,
            payload
        ));

        signer_balance -= gas_meter.spent() + fee;
        builtin_balance += fee;

        System::assert_last_event(Event::MessageQueued { message, hash }.into());

        assert_eq!(MessageNonce::get(), 1.into());
        assert_eq!(Queue::get(), queue);
        // Check that the fee was charged and transferred to the builtin
        assert_eq!(balance_of(&SIGNER), signer_balance);
        assert_eq!(balance_of(&builtin_id), builtin_balance);

        let destination = H160::random();
        let payload = H256::random().as_bytes().to_vec();

        let message = unsafe {
            EthMessage::new_unchecked(1.into(), SIGNER.cast(), destination, payload.clone())
        };
        let nonce = message.nonce();
        let hash = message.hash();

        queue.push(hash);

        let (response, _, _) = run_block_with_builtin_call(
            SIGNER,
            Request::SendEthMessage {
                destination,
                payload,
            },
            None,
            fee,
        );

        signer_balance -= gas_meter.spent() + fee;
        builtin_balance += fee;

        let response = Response::decode(&mut response.as_ref()).expect("should be `Response`");

        assert_eq!(response, Response::EthMessageQueued { nonce, hash });

        System::assert_has_event(Event::MessageQueued { message, hash }.into());

        assert_eq!(MessageNonce::get(), 2.into());
        assert_eq!(Queue::get(), queue);
        // Check that the fee was charged and transferred to the builtin
        assert_eq!(balance_of(&SIGNER), signer_balance);
        assert_eq!(balance_of(&builtin_id), builtin_balance);
    })
}

#[test]
fn bridge_queue_root_changes() {
    init_logger();
    new_test_ext().execute_with(|| {
        run_to_block(WHEN_INITIALIZED);

        assert_ok!(GearEthBridge::unpause(RuntimeOrigin::root()));

        assert!(!QueueChanged::get());

        for _ in 0..4 {
            assert_ok!(GearEthBridge::send_eth_message(
                RuntimeOrigin::signed(SIGNER),
                H160::random(),
                H256::random().as_bytes().to_vec()
            ));

            assert!(QueueChanged::get());
        }

        let expected_root = binary_merkle_tree::merkle_root::<Keccak256, _>(Queue::get());

        on_finalize_gear_block(WHEN_INITIALIZED);

        System::assert_last_event(Event::QueueMerkleRootChanged(expected_root).into());
        assert!(!QueueChanged::get());

        on_initialize(WHEN_INITIALIZED + 1);

        assert!(!QueueChanged::get());
    })
}

#[test]
fn bridge_updates_authorities_and_clears() {
    init_logger();
    new_test_ext().execute_with(|| {
        assert!(!AuthoritySetHash::exists());

        run_to_block(ERA_BLOCKS + 1);
        do_events_assertion(6, 37, None::<[_; 0]>);

        on_finalize_gear_block(ERA_BLOCKS + 1);
        do_events_assertion(
            6,
            37,
            [GrandpaEvent::NewAuthorities {
                authority_set: era_validators_authority_set(6),
            }
            .into()],
        );

        on_initialize(ERA_BLOCKS + 2);
        do_events_assertion(6, 38, []);

        assert!(!AuthoritySetHash::exists());

        assert_ok!(GearEthBridge::unpause(RuntimeOrigin::root()));

        for _ in 0..5 {
            assert_ok!(GearEthBridge::send_eth_message(
                RuntimeOrigin::signed(SIGNER),
                H160::zero(),
                vec![]
            ));
        }

        on_finalize_gear_block(ERA_BLOCKS + 2);

        assert_eq!(System::events().len(), 7);
        assert!(matches!(
            System::events().last().expect("infallible").event,
            RuntimeEvent::GearEthBridge(Event::QueueMerkleRootChanged(_))
        ));
        assert!(!QueueMerkleRoot::get().expect("infallible").is_zero());

        on_initialize(ERA_BLOCKS + 3);
        do_events_assertion(6, 39, None::<[_; 0]>);

        run_to_block(ERA_BLOCKS + EPOCH_BLOCKS * 5);
        do_events_assertion(
            10,
            66,
            [
                SessionEvent::NewSession { session_index: 7 }.into(),
                SessionEvent::NewSession { session_index: 8 }.into(),
                SessionEvent::NewSession { session_index: 9 }.into(),
                SessionEvent::NewSession { session_index: 10 }.into(),
            ],
        );

        let authority_set = era_validators_authority_set(12);
        let authority_set_ids_concat = authority_set
            .clone()
            .into_iter()
            .flat_map(|(public, _)| public.into_inner().0)
            .collect::<Vec<u8>>();
        let authority_set_hash: H256 = Blake2_256::hash(&authority_set_ids_concat).into();

        run_to_block(ERA_BLOCKS + EPOCH_BLOCKS * 5 + 1);
        do_events_assertion(
            11,
            67,
            [
                SessionEvent::NewSession { session_index: 11 }.into(),
                Event::AuthoritySetHashChanged(authority_set_hash).into(),
            ],
        );

        assert_eq!(
            AuthoritySetHash::get().expect("infallible"),
            authority_set_hash
        );
        assert!(!QueueMerkleRoot::get().expect("infallible").is_zero());

        run_to_block(ERA_BLOCKS * 2 + 1);
        do_events_assertion(
            12,
            73,
            [SessionEvent::NewSession { session_index: 12 }.into()],
        );

        on_finalize_gear_block(ERA_BLOCKS * 2 + 1);
        System::assert_last_event(GrandpaEvent::NewAuthorities { authority_set }.into());

        System::reset_events();

        on_initialize(ERA_BLOCKS * 2 + 2);
        do_events_assertion(12, 74, [Event::BridgeCleared.into()]);

        assert!(!AuthoritySetHash::exists());
        assert!(QueueMerkleRoot::get().expect("infallible").is_zero());

        run_to_block(ERA_BLOCKS * 2 + EPOCH_BLOCKS * 5);
        do_events_assertion(
            16,
            102,
            [
                SessionEvent::NewSession { session_index: 13 }.into(),
                SessionEvent::NewSession { session_index: 14 }.into(),
                SessionEvent::NewSession { session_index: 15 }.into(),
                SessionEvent::NewSession { session_index: 16 }.into(),
            ],
        );

        let authority_set = era_validators_authority_set(18);
        let authority_set_ids_concat = authority_set
            .clone()
            .into_iter()
            .flat_map(|(public, _)| public.into_inner().0)
            .collect::<Vec<u8>>();
        let authority_set_hash: H256 = Blake2_256::hash(&authority_set_ids_concat).into();

        run_to_block(ERA_BLOCKS * 2 + EPOCH_BLOCKS * 5 + 1);
        do_events_assertion(
            17,
            103,
            [
                SessionEvent::NewSession { session_index: 17 }.into(),
                Event::AuthoritySetHashChanged(authority_set_hash).into(),
            ],
        );

        run_to_block(ERA_BLOCKS * 3 + 1);
        on_finalize_gear_block(ERA_BLOCKS * 3 + 1);
        do_events_assertion(
            18,
            109,
            [
                SessionEvent::NewSession { session_index: 18 }.into(),
                GrandpaEvent::NewAuthorities { authority_set }.into(),
            ],
        );

        on_initialize(ERA_BLOCKS * 3 + 2);
        do_events_assertion(18, 110, [Event::BridgeCleared.into()]);
    })
}

#[test]
fn bridge_queues_governance_messages_when_over_capacity() {
    init_logger();
    new_test_ext().execute_with(|| {
        run_to_block(WHEN_INITIALIZED);

        assert_ok!(GearEthBridge::set_fee(
            RuntimeOrigin::root(),
            MockTransportFee::get()
        ));

        assert_ok!(GearEthBridge::unpause(RuntimeOrigin::root()));

        let queue_capacity: u32 = <Test as crate::Config>::QueueCapacity::get();

        for _ in 0..queue_capacity {
            assert_ok!(GearEthBridge::send_eth_message(
                RuntimeOrigin::signed(SIGNER),
                H160::zero(),
                vec![]
            ));
        }

        let msg_queue_len = Queue::get().len();
        assert_eq!(msg_queue_len, queue_capacity as usize);

        GearEthBridge::send_eth_message(
            RuntimeOrigin::signed(<Test as crate::Config>::BridgeAdmin::get()),
            H160::zero(),
            vec![],
        )
        .unwrap();

        assert_eq!(Queue::get().len(), msg_queue_len + 1);

        let _ = run_block_with_builtin_call(
            <Test as crate::Config>::BridgePauser::get(),
            Request::SendEthMessage {
                destination: H160::zero(),
                payload: vec![],
            },
            None,
            0,
        );

        assert_eq!(Queue::get().len(), msg_queue_len + 2);
    })
}

#[test]
fn bridge_is_not_yet_initialized_err() {
    init_logger();
    new_test_ext().execute_with(|| {
        const ERR: Error = Error::BridgeIsNotYetInitialized;

        run_to_block(1);
        run_block_and_assert_bridge_error(ERR);

        run_to_block(ERA_BLOCKS - 1);
        run_block_and_assert_bridge_error(ERR);
    })
}

#[test]
fn bridge_is_paused_err() {
    init_logger();
    new_test_ext().execute_with(|| {
        const ERR: Error = Error::BridgeIsPaused;

        run_to_block(WHEN_INITIALIZED);
        run_block_and_assert_messaging_error(
            Request::SendEthMessage {
                destination: H160::zero(),
                payload: vec![],
            },
            ERR,
        );
    })
}

#[test]
fn bridge_max_payload_size_exceeded_err() {
    init_logger();
    new_test_ext().execute_with(|| {
        const ERR: Error = Error::MaxPayloadSizeExceeded;

        run_to_block(WHEN_INITIALIZED);

        assert_ok!(GearEthBridge::unpause(RuntimeOrigin::root()));

        let max_payload_size: u32 = <Test as crate::Config>::MaxPayloadSize::get();

        run_block_and_assert_messaging_error(
            Request::SendEthMessage {
                destination: H160::zero(),
                payload: vec![0; max_payload_size as usize + 1],
            },
            ERR,
        );
    })
}

#[test]
fn bridge_queue_capacity_exceeded_err() {
    init_logger();
    new_test_ext().execute_with(|| {
        const ERR: Error = Error::QueueCapacityExceeded;

        run_to_block(WHEN_INITIALIZED);

        assert_ok!(GearEthBridge::unpause(RuntimeOrigin::root()));

        for _ in 0..<Test as crate::Config>::QueueCapacity::get() {
            assert_ok!(GearEthBridge::send_eth_message(
                RuntimeOrigin::signed(SIGNER),
                H160::zero(),
                vec![]
            ));
        }

        run_block_and_assert_messaging_error(
            Request::SendEthMessage {
                destination: H160::zero(),
                payload: vec![],
            },
            ERR,
        );
    })
}

#[test]
fn bridge_incorrect_value_applied_err() {
    init_logger();
    new_test_ext().execute_with(|| {
        run_to_block(WHEN_INITIALIZED);

        assert_ok!(GearEthBridge::set_fee(
            RuntimeOrigin::root(),
            MockTransportFee::get()
        ));
        assert_ok!(GearEthBridge::unpause(RuntimeOrigin::root()));

        let signer_balance = balance_of(&SIGNER);
        let mut gas_meter = GasSpentMeter::start();

        let (response, _, _) = run_block_with_builtin_call(
            SIGNER,
            Request::SendEthMessage {
                destination: H160::zero(),
                payload: vec![],
            },
            None,
            1,
        );

        assert_eq!(
            String::from_utf8_lossy(&response),
            format!("{}", BuiltinActorError::InsufficientValue)
        );

        // Check that value/fee was not charged
        assert_eq!(balance_of(&SIGNER), signer_balance - gas_meter.spent());
    })
}

#[test]
fn bridge_value_returned() {
    init_logger();
    new_test_ext().execute_with(|| {
        const ADDITIONAL_VALUE: u128 = 42;

        run_to_block(WHEN_INITIALIZED);

        assert_ok!(GearEthBridge::set_fee(
            RuntimeOrigin::root(),
            MockTransportFee::get()
        ));
        assert_ok!(GearEthBridge::unpause(RuntimeOrigin::root()));

        let mut gas_meter = GasSpentMeter::start();
        let mut signer_balance = balance_of(&SIGNER);
        let builtin_id = AccountId::from_origin(builtin_id().into());
        let mut builtin_balance = balance_of(&builtin_id);
        let transfer_fee = MockTransportFee::get();

        let destination = H160::random();
        let payload = H256::random().as_bytes().to_vec();

        let (response, value, err_code) = run_block_with_builtin_call(
            SIGNER,
            Request::SendEthMessage {
                destination,
                payload,
            },
            None,
            transfer_fee + ADDITIONAL_VALUE,
        );

        let _response = Response::decode(&mut response.as_ref()).expect("should be `Response`");

        signer_balance -= gas_meter.spent() + transfer_fee;
        builtin_balance += transfer_fee;

        run_for_n_blocks(40);

        assert_eq!(err_code, ReplyCode::Success(SuccessReplyReason::Manual));

        assert_eq!(value, ADDITIONAL_VALUE);
        assert_eq!(balance_of(&SIGNER), signer_balance);
        assert_eq!(balance_of(&builtin_id), builtin_balance);
    })
}

#[test]
fn bridge_insufficient_gas_err() {
    init_logger();
    new_test_ext().execute_with(|| {
        const ERR_CODE: ReplyCode = ReplyCode::Error(ErrorReplyReason::Execution(
            SimpleExecutionError::RanOutOfGas,
        ));

        run_to_block(WHEN_INITIALIZED);

        assert_ok!(GearEthBridge::unpause(RuntimeOrigin::root()));

        let (_, _, code) = run_block_with_builtin_call(
            SIGNER,
            Request::SendEthMessage {
                destination: H160::zero(),
                payload: vec![],
            },
            Some(<Test as Config>::WeightInfo::send_eth_message().ref_time() - 1),
            0,
        );

        assert_eq!(code, ERR_CODE);
    })
}

mod utils {
    use super::*;
    use crate::builtin;
    use gear_core::message::{UserMessage, Value};
    use gprimitives::MessageId;

    #[track_caller]
    pub(crate) fn run_block_and_assert_bridge_error(error: Error) {
        assert_noop!(GearEthBridge::pause(RuntimeOrigin::root()), error.clone());

        assert_noop!(GearEthBridge::unpause(RuntimeOrigin::root()), error.clone());

        run_block_and_assert_messaging_error(
            Request::SendEthMessage {
                destination: H160::zero(),
                payload: vec![],
            },
            error,
        );
    }

    #[track_caller]
    pub(crate) fn run_block_and_assert_messaging_error(request: Request, error: Error) {
        let err_str = builtin::error_to_str(&error);

        assert_noop!(
            match request.clone() {
                Request::SendEthMessage {
                    destination,
                    payload,
                } => {
                    GearEthBridge::send_eth_message(
                        RuntimeOrigin::signed(SIGNER),
                        destination,
                        payload,
                    )
                }
            },
            error
        );

        let (response, _, _) = run_block_with_builtin_call(SIGNER, request, None, 0);

        assert_eq!(String::from_utf8_lossy(&response), err_str);
    }

    #[track_caller]
    pub(crate) fn run_block_with_builtin_call(
        source: AccountId,
        request: Request,
        gas_limit: Option<u64>,
        value: u128,
    ) -> (Vec<u8>, Value, ReplyCode) {
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(source),
            builtin_id(),
            request.encode(),
            gas_limit
                .unwrap_or_else(|| <Test as Config>::WeightInfo::send_eth_message().ref_time()),
            value,
            false,
        ));

        let mid = last_message_queued();

        run_to_next_block();

        let message = last_user_message_sent();

        let reply_details = message.details().expect("must be reply");
        assert_eq!(reply_details.to_message_id(), mid);

        (
            message.payload_bytes().to_vec(),
            message.value(),
            reply_details.to_reply_code(),
        )
    }

    #[track_caller]
    pub(crate) fn last_message_queued() -> MessageId {
        System::events()
            .into_iter()
            .rev()
            .find_map(|e| {
                if let RuntimeEvent::Gear(GearEvent::MessageQueued { id, .. }) = e.event {
                    Some(id)
                } else {
                    None
                }
            })
            .expect("message queued not found")
    }

    #[track_caller]
    pub(crate) fn last_user_message_sent() -> UserMessage {
        System::events()
            .into_iter()
            .rev()
            .find_map(|e| {
                if let RuntimeEvent::Gear(GearEvent::UserMessageSent { message, .. }) = e.event {
                    Some(message)
                } else {
                    None
                }
            })
            .expect("user message sent not found")
    }

    #[track_caller]
    pub(crate) fn do_events_assertion<const N: usize>(
        session: u32,
        block_number: u64,
        events: impl Into<Option<[RuntimeEvent; N]>>,
    ) {
        assert_eq!(Session::current_index(), session);
        assert_eq!(System::block_number(), block_number);

        if let Some(events) = events.into() {
            let system_events = System::events()
                .into_iter()
                .map(|v| v.event)
                .collect::<Vec<_>>();

            assert_eq!(
                system_events,
                events.to_vec(),
                "System events count: {}, expected: {N}",
                system_events.len()
            );
        }

        System::reset_events();
    }

    pub(crate) fn balance_of(account: &AccountId) -> Value {
        Currency::free_balance(account)
    }

    pub(crate) fn balance_of_author() -> Value {
        balance_of(&Authorship::author().expect("author exist"))
    }

    pub(crate) struct GasSpentMeter {
        current: Value,
    }

    impl GasSpentMeter {
        pub(crate) fn start() -> Self {
            Self {
                current: balance_of_author(),
            }
        }

        pub(crate) fn spent(&mut self) -> Value {
            let spent = balance_of_author() - self.current;
            self.current = balance_of_author();
            spent
        }
    }
}
