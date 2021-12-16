// This file is part of Gear.

// Copyright (C) 2021 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use pallet_balances;
use frame_support::{assert_ok, assert_noop};
use frame_system::Pallet as SystemPallet;

use common::{self, IntermediateMessage, Origin as _};

use super::{pallet, Error, Event, MessageInfo, mock::{
    new_test_ext, Test, LOW_BALANCE_USER, USER_1, USER_2,}};

use utils::*;

#[test]
fn submit_program_works() {
    new_test_ext().execute_with(|| {
        let mut tm = TestManager::new(None);
        let code_kind = ProgramCodeKind::Default;

        assert!(tm.get_message_queue().is_none());
        assert_ok!(tm.submit_prog_call(USER_1, code_kind, None, None, None));

        let messages = tm
            .get_message_queue()
            .expect("There should be a message in the queue");
        assert_eq!(messages.len(), 1);

        let (msg_origin, msg_code, program_id, message_id) = match messages.into_iter().next() {
            Some(IntermediateMessage::InitProgram {
                origin,
                code,
                program_id,
                init_message_id,
                ..
            }) => (origin, code, program_id, init_message_id),
            _ => unreachable!(),
        };
        assert_eq!(msg_origin, USER_1.into_origin());
        assert_eq!(msg_code, code_kind.to_bytes());
        SystemPallet::<Test>::assert_last_event(
            Event::InitMessageEnqueued(MessageInfo {
                message_id,
                program_id,
                origin: USER_1.into_origin(),
            })
            .into(),
        )
    })
}

#[test]
fn submit_program_expected_failure() {
    new_test_ext().execute_with(|| {
        let mut tm = TestManager::new(None);

        // Insufficient account balance to reserve gas
        assert_noop!(
            tm.submit_prog_call(
                LOW_BALANCE_USER,
                ProgramCodeKind::Trapping,
                None,
                Some(10_000),
                Some(10)
            ),
            Error::<Test>::NotEnoughBalanceForReserve
        );
        // Gas limit is too high
        let block_gas_limit = <Test as pallet::Config>::BlockGasLimit::get();
        assert_noop!(
            tm.submit_prog_call(
                USER_1,
                ProgramCodeKind::Trapping,
                None,
                Some(block_gas_limit + 1),
                None
            ),
            Error::<Test>::GasLimitTooHigh
        );
    })
}

#[test]
fn submit_program_fails_on_duplicate_id() {
    new_test_ext().execute_with(|| {
        let mut tm = TestManager::new(None);

        assert_ok!(
            tm.submit_prog_call(USER_1, ProgramCodeKind::Default, None, Some(10_000), None),
        );
        // Finalize block to let queue processing run
        tm.run_to_block(2);
        // By now this program id is already in the storage
        assert_noop!(
            tm.submit_prog_call(USER_1, ProgramCodeKind::Default, None, Some(10_000), None),
            Error::<Test>::ProgramAlreadyExists
        );
    })
}

#[test]
fn send_message_works() {
    new_test_ext().execute_with(|| {
        let mut tm = TestManager::new(Some(100_000));
        let prog_id = tm.submit_prog_default_data(USER_1, ProgramCodeKind::Default);
        let payload = b"payload".to_vec();
        let expected_msg_id = tm.compute_message_id(&payload);

        assert_ok!(tm.send_msg_to_program(USER_1, prog_id, payload, None, None));

        let messages = tm
            .get_message_queue()
            .expect("There should be a message in the queue");
        assert_eq!(messages.len(), 2);
        let actual_msg_id = match messages.into_iter().next_back() {
            Some(IntermediateMessage::DispatchMessage { id, .. }) => id,
            _ => unreachable!("Last message was dispatch message"),
        };
        assert_eq!(expected_msg_id, actual_msg_id);

        assert_eq!(
            tm.get_actual_balance(USER_1),
            tm.get_expected_balance(USER_1).expect("USER_1 has balance")
        );
        assert_eq!(
            tm.get_actual_balance(USER_2),
            tm.get_expected_balance(USER_2).expect("USER_2 has balance")
        );

        assert_ok!(tm.send_msg_to_user(USER_1, USER_2, Vec::new(), None, Some(20_000)));

        assert_eq!(
            tm.get_actual_balance(USER_1),
            tm.get_expected_balance(USER_1).expect("USER_1 has balance")
        );
        tm.run_to_block(2);
        assert_eq!(
            tm.get_actual_balance(USER_1),
            tm.get_expected_balance(USER_1).expect("USER_1 has balance")
        );
        assert_eq!(
            tm.get_actual_balance(USER_2),
            tm.get_expected_balance(USER_2).expect("USER_2 has balance")
        );
    });
}

#[test]
fn send_message_expected_failure() {
    new_test_ext().execute_with(|| {
        let mut tm = TestManager::new(None);

        // Submitting failing program and check message is failed to be sent to it
        let prog_id = tm.submit_prog_default_data(USER_1, ProgramCodeKind::Trapping);
        tm.run_to_block(2);

        assert_noop!(
            tm.send_msg_to_program(LOW_BALANCE_USER, prog_id, b"payload".to_vec(), None, None),
            Error::<Test>::ProgramIsNotInitialized
        );

        // Submit valid program and test failing actions on it
        let prog_id = tm.submit_prog_default_data(USER_1, ProgramCodeKind::Default);

        assert_noop!(
            tm.send_msg_to_program(LOW_BALANCE_USER, prog_id, b"payload".to_vec(), None, None),
            Error::<Test>::NotEnoughBalanceForReserve
        );

        // Value tansfer is attempted if `value` field is greater than 0
        assert_noop!(
            tm.send_msg_to_user(
                LOW_BALANCE_USER,
                USER_1,
                b"payload".to_vec(),
                Some(1), // gas limit must be greater than 0 to have changed the state during reserve()
                Some(100)
            ),
            pallet_balances::Error::<Test>::InsufficientBalance
        );

        // Gas limit too high
        let block_gas_limit = <Test as pallet::Config>::BlockGasLimit::get();
        assert_noop!(
            tm.send_msg_to_program(
                USER_1,
                prog_id,
                b"payload".to_vec(),
                Some(block_gas_limit + 1),
                None
            ),
            Error::<Test>::GasLimitTooHigh
        );
    })
}

#[test]
fn messages_processing_works() {
    new_test_ext().execute_with(|| {
        let mut tm = TestManager::new(None);
        let wat = r#"
            (module
                (import "env" "gr_send" (func $send (param i32 i32 i32 i64 i32 i32)))
                (import "env" "memory" (memory 1))
                (export "handle" (func $handle))
                (export "init" (func $init))
                (func $handle
                    i32.const 0
                    i32.const 32
                    i32.const 32
                    i64.const 1000000000
                    i32.const 1024
                    i32.const 40000
                    call $send
                )
                (func $init)
        )"#;

        // Submit some messages to message queue
        // todo[sab] clumsy, rewrite submit_prog stuff
        let prog_id = tm.submit_prog_default_data(USER_1, ProgramCodeKind::Custom(wat));
        assert_ok!(
            tm.send_msg_to_program(
                USER_1,
                prog_id
                Vec::new(),
                None,
                None,
            )
        );
    });

    new_test_ext().execute_with(|| {
        let code = parse_wat(wat);
        let program_id = H256::from_low_u64_be(1001);

        // TODO #524
        MessageQueue::<Test>::put(vec![
            IntermediateMessage::InitProgram {
                origin: 1.into_origin(),
                code,
                program_id,
                init_message_id: H256::from_low_u64_be(1000001),
                payload: Vec::new(),
                gas_limit: 10000,
                value: 0,
            },
            IntermediateMessage::DispatchMessage {
                id: H256::from_low_u64_be(102),
                origin: 1.into_origin(),
                destination: program_id,
                payload: Vec::new(),
                gas_limit: 10000,
                value: 0,
                reply: None,
            },
        ]);
        assert_eq!(
            Gear::message_queue()
                .expect("Failed to get messages from queue")
                .len(),
            2
        );

        crate::Pallet::<Test>::process_queue();
        System::assert_last_event(crate::Event::MessagesDequeued(2).into());

        // First message is sent to a non-existing program - and should get into log.
        // Second message still gets processed thereby adding 1 to the total processed messages counter.
        MessageQueue::<Test>::put(vec![
            IntermediateMessage::DispatchMessage {
                id: H256::from_low_u64_be(102),
                origin: 1.into_origin(),
                destination: LOW_BALANCE_USER.into_origin(),
                payload: Vec::new(),
                gas_limit: 10000,
                value: 100,
                reply: None,
            },
            IntermediateMessage::DispatchMessage {
                id: H256::from_low_u64_be(103),
                origin: 1.into_origin(),
                destination: program_id,
                payload: Vec::new(),
                gas_limit: 10000,
                value: 0,
                reply: None,
            },
        ]);
        crate::Pallet::<Test>::process_queue();
        // message with log destination should never get processed
        System::assert_last_event(crate::Event::MessagesDequeued(1).into());
    })
}

// TODO [SAB]
// 1. init logger and execute with is a copy_paste - get rid of it.
// 2. origins?
// 3. 1.into_origin()? Balances::free_balance(1)? -> get rid of that nums
// 4. block_number controller, in order not to set multiple times block number in "run_to_block(Num, ..)
// 5. rewrite to make a TestBuilder
mod utils {
    use std::collections::HashMap;

    use sp_core::H256;
    use codec::Encode;
    use frame_system::Pallet as SystemPallet;
    use frame_support::{assert_ok, dispatch::DispatchResultWithPostInfo};
    use pallet_balances::Pallet as BalancePallet;

    use common::{IntermediateMessage, Origin as _};

    use crate::{
        pallet,
        Pallet as GearPallet,
        GasAllowance,
        mock::{self, Test, Origin, run_to_block, USER_1, USER_2, LOW_BALANCE_USER, BLOCK_AUTHOR},
    };

    type AccountId = <Test as frame_system::Config>::AccountId;
    type Balance = <Test as pallet_balances::Config>::Balance;

    // Program init and sending messages to programs is allowed only to USER_1
    pub(super) struct TestManager {
        global_nonce: u128,
        block_gas_limit: Option<u64>,
        balance_manager: TestBalancesManager,
    }

    struct TestBalancesManager<K = AccountId, V = Balance> {
        // map account => balance
        init_balances: HashMap<K, V>,
        // account => (gas, value). In other words, amount of tokens
        // to subtract from init balance as a result of sending message
        // to program (either init or handle)
        msg_reserve: HashMap<K, (V, V)>,
        // account => (gas, value). In other words, amount of tokens
        // to subtract from init balance as a result of sending message
        // to user from user
        mail_reserve: HashMap<K, (V, V)>,
        mail_receive: HashMap<K, V>,
    }

    #[derive(Debug, Copy, Clone)]
    pub(super) enum ProgramCodeKind<'a> {
        Default,
        Custom(&'a str),
        Trapping,
    }

    #[derive(Debug, Copy, Clone)]
    enum Receiver {
        Program(H256),
        User(AccountId),
    }

    impl TestManager {
        const DEFAULT_MSG_GAS_LIMIT: u64 = 10_000;

        pub(super) fn new(block_gas_limit: Option<u64>) -> Self {
            let _ = env_logger::Builder::from_default_env()
                .format_module_path(false)
                .format_level(true)
                .try_init();

            TestManager {
                block_gas_limit,
                global_nonce: 0,
                balance_manager: TestBalancesManager::default(),
            }
        }

        // todo [sab] what about weight?
        pub(super) fn run_to_block(&mut self, block_number: u64) {
            run_to_block(block_number, self.block_gas_limit);
            // Count actually spent gas by message sender
            let block_gas_limit = self
                .block_gas_limit
                .unwrap_or(<Test as pallet::Config>::BlockGasLimit::get());
            let gas_spent = block_gas_limit - GasAllowance::<Test>::get();
            self.balance_manager
                .update_msg_gas_reserve(gas_spent as u128);
        }

        // todo [sab] change to be sent by user 1
        pub(super) fn submit_prog_default_data(&mut self, user: u64, kind: ProgramCodeKind) -> H256 {
            assert_ok!(self.submit_prog_call(user, kind, None, None, None));
            match self
                .get_last_event()
                .expect("message was submitted previously")
            {
                mock::Event::Gear(pallet::Event::InitMessageEnqueued(msg_info)) => {
                    msg_info.program_id
                }
                _ => unreachable!(),
            }
        }

        pub(super) fn submit_prog_call(
            &mut self,
            user: u64,
            kind: ProgramCodeKind,
            payload: Option<Vec<u8>>,
            gas_limit: Option<u64>,
            value: Option<u128>,
        ) -> DispatchResultWithPostInfo {
            let gas_limit = gas_limit.unwrap_or(Self::DEFAULT_MSG_GAS_LIMIT);
            let value = value.unwrap_or_default();
            let res = GearPallet::<Test>::submit_program(
                Origin::signed(user).into(),
                kind.to_bytes(),
                b"salt".to_vec(),
                payload.unwrap_or_default(),
                gas_limit,
                value,
            );
            self.global_nonce += 1;
            self.balance_manager
                .reserve_for_message(user, gas_limit as u128, value);
            res
        }

        pub(super) fn send_msg_to_user(
            &mut self,
            user: u64,
            to: u64,
            payload: Vec<u8>,
            gas_limit: Option<u64>,
            value: Option<u128>,
        ) -> DispatchResultWithPostInfo {
            self.send_msg_call(user, Receiver::User(to), payload, gas_limit, value)
        }

        pub(super) fn send_msg_to_program(
            &mut self,
            user: u64,
            to: H256,
            payload: Vec<u8>,
            gas_limit: Option<u64>,
            value: Option<u128>,
        ) -> DispatchResultWithPostInfo {
            self.send_msg_call(user, Receiver::Program(to), payload, gas_limit, value)
        }

        // todo [sab] maybe Option<payload>
        fn send_msg_call(
            &mut self,
            from: u64,
            to: Receiver,
            payload: Vec<u8>,
            gas_limit: Option<u64>,
            value: Option<u128>,
        ) -> DispatchResultWithPostInfo {
            let gas_limit = gas_limit.unwrap_or(Self::DEFAULT_MSG_GAS_LIMIT);
            let value = value.unwrap_or_default();
            let res = GearPallet::<Test>::send_message(
                Origin::signed(from).into(),
                to.into_origin(),
                payload,
                gas_limit,
                value,
            );
            self.global_nonce += 1;
            if let Receiver::User(to) = to {
                self.balance_manager
                    .reserve_for_mail(from, to, gas_limit as u128, value);
            } else {
                self.balance_manager
                    .reserve_for_message(from, gas_limit as u128, value);
            }
            res
        }

        pub(super) fn get_actual_balance(&self, user: u64) -> u128 {
            BalancePallet::<Test>::free_balance(user)
        }

        pub(super) fn get_expected_balance(&self, user: u64) -> Option<u128> {
            self.balance_manager.compute_balance(user)
        }

        pub(super) fn get_message_queue(&self) -> Option<Vec<IntermediateMessage>> {
            GearPallet::<Test>::message_queue()
        }

        pub(super) fn compute_message_id(&self, payload: &[u8]) -> H256 {
            let mut id = payload.encode();
            id.extend_from_slice(&self.global_nonce.to_le_bytes());
            sp_io::hashing::blake2_256(&id).into()
        }

        fn get_last_event(&self) -> Option<mock::Event> {
            SystemPallet::<Test>::events()
                .last()
                .cloned()
                .map(|er| er.event)
        }
    }

    impl TestBalancesManager {
        // Cost of gas in "tokens"
        const DEFAULT_GAS_COST: Balance = 1;

        fn reserve_for_message(&mut self, user: AccountId, gas_amount: Balance, value: Balance) {
            self.reserve_common(true, user, gas_amount * self.compute_gas_cost(), value);
        }

        // todo [sab] refactor
        fn reserve_for_mail(
            &mut self,
            from: AccountId,
            to: AccountId,
            gas_amount: Balance,
            value: Balance,
        ) {
            self.reserve_common(
                false,
                from,
                gas_amount * self.compute_gas_cost(), // todo [sab] sure only for msg?
                value,
            );
            self.mail_receive
                .entry(to)
                .and_modify(|v| *v += value)
                .or_insert(value);
        }

        fn reserve_common(
            &mut self,
            is_msg_reserve: bool,
            user: AccountId,
            reserve_gas: Balance,
            reserve_value: Balance,
        ) {
            let TestBalancesManager {
                msg_reserve,
                mail_reserve,
                ..
            } = self;
            let user_reserve = if is_msg_reserve {
                msg_reserve
            } else {
                mail_reserve
            };
            user_reserve
                .entry(user)
                .and_modify(|(reserve_for_gas, reserve_value)| {
                    *reserve_for_gas = *reserve_for_gas + reserve_gas;
                    *reserve_value += *reserve_value;
                })
                .or_insert((reserve_gas, reserve_value));
        }

        // Should be changed when a better algorithm is provided
        // TODO [sab] change type
        fn compute_gas_cost(&self) -> Balance {
            Self::DEFAULT_GAS_COST as Balance
        }

        // By default user 1 - todo [sab] state in docs and make it more explicit by the code
        fn update_msg_gas_reserve(&mut self, gas_amount: Balance) {
            let msg_reserve = self.msg_reserve.get_mut(&USER_1);
            if let Some((gas_reserve, _)) = msg_reserve {
                *gas_reserve = gas_amount;
            }
        }

        fn compute_balance(&self, user: AccountId) -> Option<Balance> {
            let mut ret_balance = self.init_balances.get(&user).copied();
            if let Some(reserve_for_msgs) = self.msg_reserve.get(&user).copied().map(|(g, v)| g + v)
            {
                ret_balance.as_mut().map(|b| *b -= reserve_for_msgs);
            }
            if let Some(reserve_for_mails) =
                self.mail_reserve.get(&user).copied().map(|(g, v)| g + v)
            {
                ret_balance.as_mut().map(|b| *b -= reserve_for_mails);
            }
            if let Some(received_from_mails) = self.mail_receive.get(&user).copied() {
                ret_balance.as_mut().map(|b| *b += received_from_mails);
            }
            ret_balance
        }
    }

    impl Default for TestBalancesManager {
        fn default() -> Self {
            let mut balances = HashMap::new();
            let users = [USER_1, USER_2, LOW_BALANCE_USER, BLOCK_AUTHOR];
            for user in users {
                let balance = BalancePallet::<Test>::free_balance(user);
                balances.insert(user, balance);
            }
            TestBalancesManager {
                init_balances: balances,
                msg_reserve: HashMap::new(),
                mail_reserve: HashMap::new(),
                mail_receive: HashMap::new(),
            }
        }
    }

    impl<'a> ProgramCodeKind<'a> {
        pub(super) fn to_bytes(self) -> Vec<u8> {
            let source = match self {
                ProgramCodeKind::Default =>
                        r#"(module
                            (import "env" "memory" (memory 1))
                            (export "handle" (func $handle))
                            (export "init" (func $init))
                            (func $handle)
                            (func $init)
                        )"#,
                ProgramCodeKind::Trapping =>
                    r#"(module
                        (import "env" "memory" (memory 1))
                    )"#,
                ProgramCodeKind::Custom(code) => code,
            };

            wabt::Wat2Wasm::new()
                .validate(false)
                .convert(source)
                .expect("failed to parse module")
                .as_ref()
                .to_vec()
        }
    }

    impl Receiver {
        fn into_origin(self) -> H256 {
            match self {
                Receiver::Program(v) => v,
                Receiver::User(v) => v.into_origin(),
            }
        }
    }
}

// fn compute_code_hash(code: &[u8]) -> H256 {
//     sp_io::hashing::blake2_256(code).into()
// }
//
// fn generate_program_id(code: &[u8], salt: &[u8]) -> H256 {
//     // TODO #512
//     let mut data = Vec::new();
//     code.encode_to(&mut data);
//     salt.encode_to(&mut data);
//
//     sp_io::hashing::blake2_256(&data[..]).into()
// }
//
//
// #[test]
// fn spent_gas_to_reward_block_author_works() {
//     let wat = r#"
//     (module
//         (import "env" "gr_send" (func $send (param i32 i32 i32 i64 i32 i32)))
//         (import "env" "memory" (memory 1))
//         (export "handle" (func $handle))
//         (export "init" (func $init))
//         (func $handle
//             i32.const 0
//             i32.const 32
//             i32.const 32
//             i64.const 1000000000
//             i32.const 1024
//             i32.const 40000
//             call $send
//         )
//         (func $init
//             call $handle
//         )
//     )"#;
//
//     init_logger();
//     new_test_ext().execute_with(|| {
//         let code = parse_wat(wat);
//         let program_id = H256::from_low_u64_be(1001);
//
//         let init_message_id = H256::from_low_u64_be(1000001);
//         // TODO #524
//         MessageQueue::<Test>::put(vec![IntermediateMessage::InitProgram {
//             origin: 1.into_origin(),
//             code,
//             program_id,
//             init_message_id,
//             payload: "init".as_bytes().to_vec(),
//             gas_limit: 10000,
//             value: 0,
//         }]);
//
//         let block_author_initial_balance = Balances::free_balance(BLOCK_AUTHOR);
//
//         crate::Pallet::<Test>::process_queue();
//         System::assert_last_event(crate::Event::MessagesDequeued(1).into());
//
//         // The block author should be paid the amount of Currency equal to
//         // the `gas_charge` incurred while processing the `InitProgram` message
//         assert_eq!(
//             Balances::free_balance(BLOCK_AUTHOR),
//             block_author_initial_balance.saturating_add(6_000)
//         );
//     })
// }
//
// #[test]
// fn unused_gas_released_back_works() {
//     let wat = r#"
//     (module
//         (import "env" "gr_send" (func $send (param i32 i32 i32 i64 i32 i32)))
//         (import "env" "memory" (memory 1))
//         (export "handle" (func $handle))
//         (export "init" (func $init))
//         (func $handle
//             i32.const 0
//             i32.const 32
//             i32.const 32
//             i64.const 1000000000
//             i32.const 1024
//             i32.const 40000
//             call $send
//         )
//         (func $init)
//     )"#;
//
//     init_logger();
//     new_test_ext().execute_with(|| {
//         let code = parse_wat(wat);
//         let program_id = H256::from_low_u64_be(1001);
//
//         // TODO #524
//         MessageQueue::<Test>::put(vec![IntermediateMessage::InitProgram {
//             origin: 1.into_origin(),
//             code,
//             program_id,
//             init_message_id: H256::from_low_u64_be(1000001),
//             payload: "init".as_bytes().to_vec(),
//             gas_limit: 5000_u64,
//             value: 0_u128,
//         }]);
//         crate::Pallet::<Test>::process_queue();
//
//         let external_origin_initial_balance = Balances::free_balance(1);
//         assert_ok!(Pallet::<Test>::send_message(
//             Origin::signed(1).into(),
//             program_id,
//             Vec::new(),
//             20_000_u64,
//             0_u128,
//         ));
//         // send_message reserves balance on the sender's account
//         assert_eq!(
//             Balances::free_balance(1),
//             external_origin_initial_balance.saturating_sub(20_000)
//         );
//
//         crate::Pallet::<Test>::process_queue();
//
//         // Unused gas should be converted back to currency and released to the external origin
//         assert_eq!(
//             Balances::free_balance(1),
//             external_origin_initial_balance.saturating_sub(10_000)
//         );
//     })
// }
//
// fn init_test_program(origin: H256, program_id: H256, wat: &str) {
//     let code = parse_wat(wat);
//     // TODO #524
//     MessageQueue::<Test>::put(vec![IntermediateMessage::InitProgram {
//         origin,
//         code,
//         program_id,
//         init_message_id: H256::from_low_u64_be(1000001),
//         payload: "init".as_bytes().to_vec(),
//         gas_limit: 10_000_000_u64,
//         value: 0_u128,
//     }]);
//
//     crate::Pallet::<Test>::process_queue();
// }
//
// #[test]
// fn block_gas_limit_works() {
//     // A module with $handle function being worth 6000 gas
//     let wat1 = r#"
// 	(module
// 		(import "env" "gr_send" (func $send (param i32 i32 i32 i64 i32 i32)))
// 		(import "env" "memory" (memory 1))
// 		(export "handle" (func $handle))
// 		(export "init" (func $init))
// 		(func $handle
// 			i32.const 0
// 			i32.const 32
// 			i32.const 32
// 			i64.const 1000000000
// 			i32.const 1024
// 			i32.const 40000
// 			call $send
// 		)
// 		(func $init)
// 	)"#;
//
//     // A module with $handle function being worth 94000 gas
//     let wat2 = r#"
// 	(module
// 		(import "env" "memory" (memory 1))
// 		(export "handle" (func $handle))
// 		(export "init" (func $init))
// 		(func $init)
//         (func $doWork (param $size i32)
//             (local $counter i32)
//             i32.const 0
//             set_local $counter
//             loop $while
//                 get_local $counter
//                 i32.const 1
//                 i32.add
//                 set_local $counter
//                 get_local $counter
//                 get_local $size
//                 i32.lt_s
//                 if
//                     br $while
//                 end
//             end $while
//         )
//         (func $handle
//             i32.const 10
//             call $doWork
// 		)
// 	)"#;
//
//     init_logger();
//     new_test_ext().execute_with(|| {
//         let code1 = parse_wat(wat1);
//         let code2 = parse_wat(wat2);
//         let pid1 = H256::from_low_u64_be(1001);
//         let pid2 = H256::from_low_u64_be(1002);
//
//         // TODO #524
//         MessageQueue::<Test>::put(vec![
//             IntermediateMessage::InitProgram {
//                 origin: 1.into_origin(),
//                 code: code1,
//                 program_id: pid1,
//                 init_message_id: H256::from_low_u64_be(1000001),
//                 payload: Vec::new(),
//                 gas_limit: 10_000,
//                 value: 0,
//             },
//             IntermediateMessage::DispatchMessage {
//                 id: H256::from_low_u64_be(102),
//                 origin: 1.into_origin(),
//                 destination: pid1,
//                 payload: Vec::new(),
//                 gas_limit: 10_000,
//                 value: 0,
//                 reply: None,
//             },
//             IntermediateMessage::DispatchMessage {
//                 id: H256::from_low_u64_be(103),
//                 origin: 1.into_origin(),
//                 destination: pid1,
//                 payload: Vec::new(),
//                 gas_limit: 10_000,
//                 value: 100,
//                 reply: None,
//             },
//             IntermediateMessage::InitProgram {
//                 origin: 1.into_origin(),
//                 code: code2,
//                 program_id: pid2,
//                 init_message_id: H256::from_low_u64_be(1000002),
//                 payload: Vec::new(),
//                 gas_limit: 10_000,
//                 value: 0,
//             },
//         ]);
//
//         // Run to block #2 where the queue processing takes place
//         run_to_block(2, Some(100_000));
//         System::assert_last_event(crate::Event::MessagesDequeued(4).into());
//
//         // Run to the next block to reset the gas limit
//         run_to_block(3, Some(100_000));
//
//         assert!(MessageQueue::<Test>::get().is_none());
//
//         // Add more messages to queue
//         // Total `gas_limit` of three messages exceeds the block gas limit
//         // Messages #1 abd #3 take 6000 gas
//         // Message #2 takes 94000 gas
//         MessageQueue::<Test>::put(vec![
//             IntermediateMessage::DispatchMessage {
//                 id: H256::from_low_u64_be(104),
//                 origin: 1.into_origin(),
//                 destination: pid1,
//                 payload: Vec::new(),
//                 gas_limit: 10_000,
//                 value: 0,
//                 reply: None,
//             },
//             IntermediateMessage::DispatchMessage {
//                 id: H256::from_low_u64_be(105),
//                 origin: 1.into_origin(),
//                 destination: pid2,
//                 payload: Vec::new(),
//                 gas_limit: 95_000,
//                 value: 100,
//                 reply: None,
//             },
//             IntermediateMessage::DispatchMessage {
//                 id: H256::from_low_u64_be(106),
//                 origin: 1.into_origin(),
//                 destination: pid1,
//                 payload: Vec::new(),
//                 gas_limit: 20_000,
//                 value: 200,
//                 reply: None,
//             },
//         ]);
//
//         run_to_block(4, Some(100_000));
//
//         // Message #2 steps beyond the block gas allowance and is requeued
//         // Message #1 is dequeued and processed, message #3 stays in the queue:
//         //
//         // | 1 |        | 3 |
//         // | 2 |  ===>  | 2 |
//         // | 3 |        |   |
//         //
//         System::assert_last_event(crate::Event::MessagesDequeued(1).into());
//         assert_eq!(Gear::gas_allowance(), 90_000);
//
//         // Run to the next block to reset the gas limit
//         run_to_block(5, Some(100_000));
//
//         // Message #3 get dequeued and processed
//         // Message #2 gas limit still exceeds the remaining allowance:
//         //
//         // | 3 |        | 2 |
//         // | 2 |  ===>  |   |
//         //
//         System::assert_last_event(crate::Event::MessagesDequeued(1).into());
//         assert_eq!(Gear::gas_allowance(), 90_000);
//
//         run_to_block(6, Some(100_000));
//
//         // This time message #2 makes it into the block:
//         //
//         // | 2 |        |   |
//         // |   |  ===>  |   |
//         //
//         System::assert_last_event(crate::Event::MessagesDequeued(1).into());
//         assert_eq!(Gear::gas_allowance(), 11_000);
//     });
// }
//
// #[test]
// fn mailbox_works() {
//     let wat = r#"
//     (module
//         (import "env" "gr_send" (func $send (param i32 i32 i32 i64 i32 i32)))
//         (import "env" "gr_source" (func $gr_source (param i32)))
//         (import "env" "memory" (memory 1))
//         (export "handle" (func $handle))
//         (export "init" (func $init))
//         (export "handle_reply" (func $handle_reply))
//         (func $handle
//             i32.const 16384
//             call $gr_source
//             i32.const 16384
//             i32.const 0
//             i32.const 32
//             i64.const 1000000
//             i32.const 1024
//             i32.const 40000
//             call $send
//         )
//         (func $handle_reply)
//         (func $init)
//     )"#;
//
//     init_logger();
//     new_test_ext().execute_with(|| {
//         let program_id = H256::from_low_u64_be(1001);
//
//         init_test_program(1.into_origin(), program_id, wat);
//
//         assert_ok!(Pallet::<Test>::send_message(
//             Origin::signed(1).into(),
//             program_id,
//             Vec::new(),
//             2_000_000_u64,
//             0_u128,
//         ));
//         crate::Pallet::<Test>::process_queue();
//
//         let mailbox_message = crate::Pallet::<Test>::remove_from_mailbox(
//             1.into_origin(),
//             // this is fixed (nonce based)
//             hex!("211a310ae0d68d7a4523ccecc7e5c0fd435496008c56ba8c86c5bba45d466e3a").into(),
//         )
//         .expect("There should be a message for user #1 in the mailbox");
//
//         assert_eq!(
//             mailbox_message.id,
//             hex!("211a310ae0d68d7a4523ccecc7e5c0fd435496008c56ba8c86c5bba45d466e3a").into(),
//         );
//
//         assert_eq!(mailbox_message.payload, vec![0u8; 32]);
//
//         assert_eq!(mailbox_message.gas_limit, 1000000);
//     })
// }
//
// #[test]
// fn init_message_logging_works() {
//     let wat1 = r#"
//     (module
//         (import "env" "memory" (memory 1))
//         (export "init" (func $init))
//         (func $init)
//     )"#;
//
//     let wat2 = r#"
// 	(module
// 		(import "env" "memory" (memory 1))
// 		(export "init" (func $init))
//         (func $doWork (param $size i32)
//             (local $counter i32)
//             i32.const 0
//             set_local $counter
//             loop $while
//                 get_local $counter
//                 i32.const 1
//                 i32.add
//                 set_local $counter
//                 get_local $counter
//                 get_local $size
//                 i32.lt_s
//                 if
//                     br $while
//                 end
//             end $while
//         )
//         (func $init
//             i32.const 4
//             call $doWork
// 		)
// 	)"#;
//
//     init_logger();
//     new_test_ext().execute_with(|| {
//         let code = parse_wat(wat1);
//
//         System::reset_events();
//
//         assert_ok!(Pallet::<Test>::submit_program(
//             Origin::signed(1).into(),
//             code.clone(),
//             b"salt".to_vec(),
//             Vec::new(),
//             10_000u64,
//             0_u128
//         ));
//
//         let messages: Vec<IntermediateMessage> =
//             Gear::message_queue().expect("There should be a message in the queue");
//
//         let (program_id, message_id) = match &messages[0] {
//             IntermediateMessage::InitProgram {
//                 program_id,
//                 init_message_id,
//                 ..
//             } => (*program_id, *init_message_id),
//             _ => Default::default(),
//         };
//         System::assert_last_event(
//             crate::Event::InitMessageEnqueued(crate::MessageInfo {
//                 message_id,
//                 program_id,
//                 origin: 1.into_origin(),
//             })
//             .into(),
//         );
//
//         run_to_block(2, None);
//
//         // Expecting the log to have an InitSuccess event
//         System::assert_has_event(
//             crate::Event::InitSuccess(crate::MessageInfo {
//                 message_id,
//                 program_id,
//                 origin: 1.into_origin(),
//             })
//             .into(),
//         );
//
//         let code = parse_wat(wat2);
//         System::reset_events();
//         assert_ok!(Pallet::<Test>::submit_program(
//             Origin::signed(1).into(),
//             code.clone(),
//             b"salt".to_vec(),
//             Vec::new(),
//             10_000u64,
//             0_u128
//         ));
//
//         let messages: Vec<IntermediateMessage> =
//             Gear::message_queue().expect("There should be a message in the queue");
//
//         let (program_id, message_id) = match &messages[0] {
//             IntermediateMessage::InitProgram {
//                 program_id,
//                 init_message_id,
//                 ..
//             } => (*program_id, *init_message_id),
//             _ => Default::default(),
//         };
//         System::assert_last_event(
//             crate::Event::InitMessageEnqueued(crate::MessageInfo {
//                 message_id,
//                 program_id,
//                 origin: 1.into_origin(),
//             })
//             .into(),
//         );
//
//         run_to_block(3, None);
//
//         // Expecting the log to have an InitFailure event (due to insufficient gas)
//         System::assert_has_event(
//             crate::Event::InitFailure(
//                 crate::MessageInfo {
//                     message_id,
//                     program_id,
//                     origin: 1.into_origin(),
//                 },
//                 crate::Reason::Dispatch(hex!("48476173206c696d6974206578636565646564").into()),
//             )
//             .into(),
//         );
//     })
// }
//
// #[test]
// fn program_lifecycle_works() {
//     let wat1 = r#"
//     (module
//         (import "env" "memory" (memory 1))
//         (export "init" (func $init))
//         (func $init)
//     )"#;
//
//     let wat2 = r#"
// 	(module
// 		(import "env" "memory" (memory 1))
// 		(export "init" (func $init))
//         (func $doWork (param $size i32)
//             (local $counter i32)
//             i32.const 0
//             set_local $counter
//             loop $while
//                 get_local $counter
//                 i32.const 1
//                 i32.add
//                 set_local $counter
//                 get_local $counter
//                 get_local $size
//                 i32.lt_s
//                 if
//                     br $while
//                 end
//             end $while
//         )
//         (func $init
//             i32.const 4
//             call $doWork
// 		)
// 	)"#;
//
//     init_logger();
//     new_test_ext().execute_with(|| {
//         let code = parse_wat(wat1);
//
//         System::reset_events();
//
//         assert_ok!(Pallet::<Test>::submit_program(
//             Origin::signed(1).into(),
//             code.clone(),
//             b"salt".to_vec(),
//             Vec::new(),
//             10_000u64,
//             0_u128
//         ));
//
//         let messages: Vec<IntermediateMessage> =
//             Gear::message_queue().expect("There should be a message in the queue");
//         let program_id = match &messages[0] {
//             IntermediateMessage::InitProgram { program_id, .. } => *program_id,
//             _ => Default::default(),
//         };
//         assert!(common::get_program(program_id).is_none());
//         run_to_block(2, None);
//         // Expect the program to be in PS by now
//         assert!(common::get_program(program_id).is_some());
//
//         // Submitting another program
//         let code = parse_wat(wat2);
//         System::reset_events();
//         assert_ok!(Pallet::<Test>::submit_program(
//             Origin::signed(1).into(),
//             code.clone(),
//             b"salt".to_vec(),
//             Vec::new(),
//             10_000u64,
//             0_u128
//         ));
//
//         let messages: Vec<IntermediateMessage> =
//             Gear::message_queue().expect("There should be a message in the queue");
//         let program_id = match &messages[0] {
//             IntermediateMessage::InitProgram { program_id, .. } => *program_id,
//             _ => Default::default(),
//         };
//
//         assert!(common::get_program(program_id).is_none());
//         run_to_block(3, None);
//         // Expect the program to have made it to the PS
//         assert!(common::get_program(program_id).is_some());
//         // while at the same time being stuck in "limbo"
//         assert!(crate::Pallet::<Test>::is_uninitialized(program_id));
//         assert_eq!(
//             ProgramsLimbo::<Test>::get(program_id).unwrap(),
//             1.into_origin()
//         );
//         // Program author is allowed to remove the program and reclaim funds
//         // An attempt to remove a program on behalf of another account will fail
//         assert_ok!(Pallet::<Test>::remove_stale_program(
//             Origin::signed(LOW_BALANCE_USER).into(), // Not the author
//             program_id,
//         ));
//         // Program is still in the storage
//         assert!(common::get_program(program_id).is_some());
//         assert!(ProgramsLimbo::<Test>::get(program_id).is_some());
//
//         assert_ok!(Pallet::<Test>::remove_stale_program(
//             Origin::signed(1).into(),
//             program_id,
//         ));
//         // This time the program has been removed
//         assert!(common::get_program(program_id).is_none());
//         assert!(ProgramsLimbo::<Test>::get(program_id).is_none());
//     })
// }
//
// #[test]
// fn events_logging_works() {
//     let wat_ok = r#"
// 	(module
// 		(import "env" "gr_send" (func $send (param i32 i32 i32 i64 i32 i32)))
// 		(import "env" "memory" (memory 1))
// 		(export "handle" (func $handle))
// 		(export "init" (func $init))
// 		(func $handle
// 			i32.const 0
// 			i32.const 32
// 			i32.const 32
// 			i64.const 1000000
// 			i32.const 1024
//             i32.const 40000
// 			call $send
// 		)
// 		(func $init)
// 	)"#;
//
//     let wat_greedy_init = r#"
// 	(module
// 		(import "env" "memory" (memory 1))
// 		(export "init" (func $init))
//         (func $doWork (param $size i32)
//             (local $counter i32)
//             i32.const 0
//             set_local $counter
//             loop $while
//                 get_local $counter
//                 i32.const 1
//                 i32.add
//                 set_local $counter
//                 get_local $counter
//                 get_local $size
//                 i32.lt_s
//                 if
//                     br $while
//                 end
//             end $while
//         )
//         (func $init
//             i32.const 4
//             call $doWork
// 		)
// 	)"#;
//
//     let wat_trap_in_handle = r#"
// 	(module
// 		(import "env" "memory" (memory 1))
// 		(export "handle" (func $handle))
// 		(export "init" (func $init))
// 		(func $handle
// 			unreachable
// 		)
// 		(func $init)
// 	)"#;
//
//     let wat_trap_in_init = r#"
// 	(module
// 		(import "env" "memory" (memory 1))
// 		(export "handle" (func $handle))
// 		(export "init" (func $init))
// 		(func $handle)
// 		(func $init
//             unreachable
//         )
// 	)"#;
//
//     init_logger();
//     new_test_ext().execute_with(|| {
//         let code_ok = parse_wat(wat_ok);
//         let code_greedy_init = parse_wat(wat_greedy_init);
//         let code_trap_in_init = parse_wat(wat_trap_in_init);
//         let code_trap_in_handle = parse_wat(wat_trap_in_handle);
//
//         System::reset_events();
//
//         // init ok
//         assert_ok!(Pallet::<Test>::submit_program(
//             Origin::signed(1).into(),
//             code_ok.clone(),
//             b"0001".to_vec(),
//             vec![],
//             10_000u64,
//             0_u128
//         ));
//         // init out-of-gas
//         assert_ok!(Pallet::<Test>::submit_program(
//             Origin::signed(1).into(),
//             code_greedy_init.clone(),
//             b"0002".to_vec(),
//             vec![],
//             10_000u64,
//             0_u128
//         ));
//         // init trapped
//         assert_ok!(Pallet::<Test>::submit_program(
//             Origin::signed(1).into(),
//             code_trap_in_init.clone(),
//             b"0003".to_vec(),
//             vec![],
//             10_000u64,
//             0_u128
//         ));
//         // init ok
//         assert_ok!(Pallet::<Test>::submit_program(
//             Origin::signed(1).into(),
//             code_trap_in_handle.clone(),
//             b"0004".to_vec(),
//             vec![],
//             10_000u64,
//             0_u128
//         ));
//
//         let messages: Vec<IntermediateMessage> =
//             Gear::message_queue().expect("There should be a message in the queue");
//
//         let mut init_msg = vec![];
//         for message in messages {
//             match message {
//                 IntermediateMessage::InitProgram {
//                     program_id,
//                     init_message_id,
//                     ..
//                 } => {
//                     init_msg.push((init_message_id, program_id));
//                     System::assert_has_event(
//                         crate::Event::InitMessageEnqueued(crate::MessageInfo {
//                             message_id: init_message_id,
//                             program_id,
//                             origin: 1.into_origin(),
//                         })
//                         .into(),
//                     );
//                 }
//                 _ => (),
//             }
//         }
//         assert_eq!(init_msg.len(), 4);
//
//         run_to_block(2, None);
//
//         // Expecting programs 1 and 4 to have been inited successfully
//         System::assert_has_event(
//             crate::Event::InitSuccess(crate::MessageInfo {
//                 message_id: init_msg[0].0,
//                 program_id: init_msg[0].1,
//                 origin: 1.into_origin(),
//             })
//             .into(),
//         );
//         System::assert_has_event(
//             crate::Event::InitSuccess(crate::MessageInfo {
//                 message_id: init_msg[3].0,
//                 program_id: init_msg[3].1,
//                 origin: 1.into_origin(),
//             })
//             .into(),
//         );
//
//         // Expecting programs 2 and 3 to have failed to init
//         System::assert_has_event(
//             crate::Event::InitFailure(
//                 crate::MessageInfo {
//                     message_id: init_msg[1].0,
//                     program_id: init_msg[1].1,
//                     origin: 1.into_origin(),
//                 },
//                 crate::Reason::Dispatch(hex!("48476173206c696d6974206578636565646564").into()),
//             )
//             .into(),
//         );
//         System::assert_has_event(
//             crate::Event::InitFailure(
//                 crate::MessageInfo {
//                     message_id: init_msg[2].0,
//                     program_id: init_msg[2].1,
//                     origin: 1.into_origin(),
//                 },
//                 crate::Reason::Dispatch(vec![]),
//             )
//             .into(),
//         );
//
//         System::reset_events();
//
//         // Sending messages to failed-to-init programs shouldn't be allowed
//         assert_noop!(
//             Pallet::<Test>::send_message(
//                 Origin::signed(1).into(),
//                 init_msg[1].1,
//                 vec![],
//                 10_000_u64,
//                 0_u128
//             ),
//             Error::<Test>::ProgramIsNotInitialized
//         );
//         assert_noop!(
//             Pallet::<Test>::send_message(
//                 Origin::signed(1).into(),
//                 init_msg[2].1,
//                 vec![],
//                 10_000_u64,
//                 0_u128
//             ),
//             Error::<Test>::ProgramIsNotInitialized
//         );
//
//         // Messages to fully-initialized programs are accepted
//         assert_ok!(Pallet::<Test>::send_message(
//             Origin::signed(1).into(),
//             init_msg[0].1,
//             vec![],
//             10_000_000_u64,
//             0_u128
//         ));
//         assert_ok!(Pallet::<Test>::send_message(
//             Origin::signed(1).into(),
//             init_msg[3].1,
//             vec![],
//             10_000_u64,
//             0_u128
//         ));
//
//         let messages: Vec<IntermediateMessage> =
//             Gear::message_queue().expect("There should be a message in the queue");
//
//         let mut dispatch_msg = vec![];
//         for message in messages {
//             match message {
//                 IntermediateMessage::DispatchMessage {
//                     id,
//                     destination,
//                     origin,
//                     ..
//                 } => {
//                     dispatch_msg.push(id);
//                     System::assert_has_event(
//                         crate::Event::DispatchMessageEnqueued(crate::MessageInfo {
//                             message_id: id,
//                             program_id: destination,
//                             origin,
//                         })
//                         .into(),
//                     );
//                 }
//                 _ => (),
//             }
//         }
//         assert_eq!(dispatch_msg.len(), 2);
//
//         run_to_block(3, None);
//
//         // First program completed successfully
//         System::assert_has_event(
//             crate::Event::MessageDispatched(DispatchOutcome {
//                 message_id: dispatch_msg[0],
//                 outcome: ExecutionResult::Success,
//             })
//             .into(),
//         );
//         // Fourth program failed to handle message
//         System::assert_has_event(
//             crate::Event::MessageDispatched(DispatchOutcome {
//                 message_id: dispatch_msg[1],
//                 outcome: ExecutionResult::Failure(vec![]),
//             })
//             .into(),
//         );
//     })
// }
//
// #[test]
// fn send_reply_works() {
//     init_logger();
//
//     new_test_ext().execute_with(|| {
//         // Make sure we have a program in the program storage
//         let program_id = H256::from_low_u64_be(1001);
//         let program = Program::new(
//             ProgramId::from_slice(&program_id[..]),
//             parse_wat(
//                 r#"(module
//                     (import "env" "memory" (memory 1))
//                     (export "handle" (func $handle))
//                     (func $handle)
//                 )"#,
//             ),
//             Default::default(),
//         )
//         .unwrap();
//         common::native::set_program(program);
//
//         let original_message_id = H256::from_low_u64_be(2002);
//         Gear::insert_to_mailbox(
//             1.into_origin(),
//             common::Message {
//                 id: original_message_id.clone(),
//                 source: program_id.clone(),
//                 dest: 1.into_origin(),
//                 payload: vec![],
//                 gas_limit: 10_000_000_u64,
//                 value: 0_u128,
//                 reply: None,
//             },
//         );
//
//         assert_ok!(Pallet::<Test>::send_reply(
//             Origin::signed(1).into(),
//             original_message_id,
//             b"payload".to_vec(),
//             10_000_000_u64,
//             0_u128
//         ));
//
//         let messages: Vec<IntermediateMessage> =
//             Gear::message_queue().expect("There should be a message in the queue");
//         assert_eq!(messages.len(), 1);
//
//         let mut id = b"payload".to_vec().encode();
//         id.extend_from_slice(&0_u128.to_le_bytes());
//         let id: H256 = sp_io::hashing::blake2_256(&id).into();
//
//         let (msg_id, orig_id) = match &messages[0] {
//             IntermediateMessage::DispatchMessage { id, reply, .. } => (*id, reply.unwrap()),
//             _ => Default::default(),
//         };
//         assert_eq!(msg_id, id);
//         assert_eq!(orig_id, original_message_id);
//     })
// }
//
// #[test]
// fn send_reply_expected_failure() {
//     init_logger();
//     new_test_ext().execute_with(|| {
//         let program_id = H256::from_low_u64_be(1001);
//         let program = Program::new(
//             ProgramId::from_slice(&program_id[..]),
//             parse_wat(
//                 r#"(module
//                     (import "env" "memory" (memory 1))
//                 )"#,
//             ),
//             Default::default(),
//         )
//         .expect("Program failed to instantiate");
//         common::native::set_program(program);
//
//         let original_message_id = H256::from_low_u64_be(2002);
//
//         // Expecting error as long as the user doesn't have messages in mailbox
//         assert_noop!(
//             Pallet::<Test>::send_reply(
//                 Origin::signed(LOW_BALANCE_USER).into(),
//                 original_message_id,
//                 b"payload".to_vec(),
//                 10_000_u64,
//                 0_u128
//             ),
//             Error::<Test>::NoMessageInMailbox
//         );
//
//         Gear::insert_to_mailbox(
//             LOW_BALANCE_USER.into_origin(),
//             common::Message {
//                 id: original_message_id,
//                 source: program_id.clone(),
//                 dest: LOW_BALANCE_USER.into_origin(),
//                 payload: vec![],
//                 gas_limit: 10_000_000_u64,
//                 value: 0_u128,
//                 reply: None,
//             },
//         );
//
//         assert_noop!(
//             Pallet::<Test>::send_reply(
//                 Origin::signed(LOW_BALANCE_USER).into(),
//                 original_message_id,
//                 b"payload".to_vec(),
//                 10_000_003_u64,
//                 0_u128
//             ),
//             Error::<Test>::NotEnoughBalanceForReserve
//         );
//
//         // Value tansfer is attempted if `value` field is greater than 0
//         assert_noop!(
//             Pallet::<Test>::send_reply(
//                 Origin::signed(LOW_BALANCE_USER).into(),
//                 original_message_id,
//                 b"payload".to_vec(),
//                 10_000_001_u64, // Must be greater than incoming gas_limit to have changed the state during reserve()
//                 100_u128,
//             ),
//             pallet_balances::Error::<Test>::InsufficientBalance
//         );
//
//         // Gas limit too high
//         assert_noop!(
//             Pallet::<Test>::send_reply(
//                 Origin::signed(1).into(),
//                 original_message_id,
//                 b"payload".to_vec(),
//                 100_000_001_u64,
//                 0_u128
//             ),
//             Error::<Test>::GasLimitTooHigh
//         );
//     })
// }
//
// #[test]
// fn send_reply_value_offset_works() {
//     init_logger();
//     new_test_ext().execute_with(|| {
//         let program_id = H256::from_low_u64_be(1001);
//         let program = Program::new(
//             ProgramId::from_slice(&program_id[..]),
//             parse_wat(
//                 r#"(module
//                     (import "env" "memory" (memory 1))
//                 )"#,
//             ),
//             Default::default(),
//         )
//         .expect("Program failed to instantiate");
//         common::native::set_program(program);
//
//         let original_message_id = H256::from_low_u64_be(2002);
//
//         Gear::insert_to_mailbox(
//             1.into_origin(),
//             common::Message {
//                 id: original_message_id,
//                 source: program_id.clone(),
//                 dest: 1.into_origin(),
//                 payload: vec![],
//                 gas_limit: 10_000_000_u64,
//                 value: 1_000_u128,
//                 reply: None,
//             },
//         );
//
//         // Program doesn't have enough balance - error expected
//         assert_noop!(
//             Pallet::<Test>::send_reply(
//                 Origin::signed(1).into(),
//                 original_message_id,
//                 b"payload".to_vec(),
//                 10_000_000_u64,
//                 0_u128
//             ),
//             pallet_balances::Error::<Test>::InsufficientBalance
//         );
//
//         assert_ok!(
//             <<Test as crate::Config>::Currency as Currency<_>>::transfer(
//                 &1,
//                 &<<Test as frame_system::Config>::AccountId as common::Origin>::from_origin(
//                     program_id
//                 ),
//                 20_000_000,
//                 ExistenceRequirement::AllowDeath,
//             )
//         );
//         assert_eq!(Balances::free_balance(1), 80_000_000);
//         assert_eq!(Balances::reserved_balance(1), 0);
//
//         assert_ok!(Pallet::<Test>::send_reply(
//             Origin::signed(1).into(),
//             original_message_id,
//             b"payload".to_vec(),
//             1_000_000_u64,
//             100_u128,
//         ));
//         assert_eq!(Balances::free_balance(1), 89_000_900);
//         assert_eq!(Balances::reserved_balance(1), 0);
//
//         Gear::remove_from_mailbox(1.into_origin(), original_message_id);
//         Gear::insert_to_mailbox(
//             1.into_origin(),
//             common::Message {
//                 id: original_message_id,
//                 source: program_id.clone(),
//                 dest: 1.into_origin(),
//                 payload: vec![],
//                 gas_limit: 10_000_000_u64,
//                 value: 1_000_u128,
//                 reply: None,
//             },
//         );
//         assert_ok!(Pallet::<Test>::send_reply(
//             Origin::signed(1).into(),
//             original_message_id,
//             b"payload".to_vec(),
//             20_000_000_u64,
//             2_000_u128,
//         ));
//         assert_eq!(Balances::free_balance(1), 78_999_900);
//         assert_eq!(Balances::reserved_balance(1), 10_000_000);
//     })
// }
//
// #[test]
// fn claim_value_from_mailbox_works() {
//     init_logger();
//     new_test_ext().execute_with(|| {
//         let program_id = H256::from_low_u64_be(1001);
//         let program = Program::new(
//             ProgramId::from_slice(&program_id[..]),
//             parse_wat(
//                 r#"(module
//                     (import "env" "memory" (memory 1))
//                 )"#,
//             ),
//             Default::default(),
//         )
//         .expect("Program failed to instantiate");
//         common::native::set_program(program);
//
//         let original_message_id = H256::from_low_u64_be(2002);
//         common::value_tree::ValueView::get_or_create(
//             GAS_VALUE_PREFIX,
//             1.into_origin(),
//             original_message_id.clone(),
//             10_000_000,
//         );
//
//         Gear::insert_to_mailbox(
//             1.into_origin(),
//             common::Message {
//                 id: original_message_id,
//                 source: program_id.clone(),
//                 dest: 1.into_origin(),
//                 payload: vec![],
//                 gas_limit: 10_000_000_u64,
//                 value: 1_000_u128,
//                 reply: None,
//             },
//         );
//
//         // Program doesn't have enough balance - error expected
//         assert_noop!(
//             Pallet::<Test>::send_reply(
//                 Origin::signed(1).into(),
//                 original_message_id,
//                 b"payload".to_vec(),
//                 10_000_000_u64,
//                 0_u128
//             ),
//             pallet_balances::Error::<Test>::InsufficientBalance
//         );
//
//         assert_ok!(
//             <<Test as crate::Config>::Currency as Currency<_>>::transfer(
//                 &1,
//                 &<<Test as frame_system::Config>::AccountId as common::Origin>::from_origin(
//                     program_id
//                 ),
//                 20_000_000,
//                 ExistenceRequirement::AllowDeath,
//             )
//         );
//         assert_eq!(Balances::free_balance(1), 80_000_000);
//         assert_eq!(Balances::reserved_balance(1), 0);
//
//         assert_ok!(Pallet::<Test>::claim_value_from_mailbox(
//             Origin::signed(1).into(),
//             original_message_id,
//         ));
//         assert_eq!(Balances::free_balance(1), 80_001_000);
//         assert_eq!(Balances::reserved_balance(1), 0);
//
//         System::assert_last_event(
//             crate::Event::ClaimedValueFromMailbox(original_message_id).into(),
//         );
//     })
// }
//
// #[test]
// fn distributor_initialize() {
//     use tests_distributor::WASM_BINARY_BLOATY;
//
//     new_test_ext().execute_with(|| {
//         let initial_balance = Balances::free_balance(1) + Balances::free_balance(255);
//
//         Pallet::<Test>::submit_program(
//             Origin::signed(1).into(),
//             WASM_BINARY_BLOATY.expect("Wasm binary missing!").to_vec(),
//             vec![],
//             vec![],
//             10_000_000_u64,
//             0_u128,
//         )
//         .expect("Submit program failed");
//
//         run_to_block(3, None);
//
//         let final_balance = Balances::free_balance(1) + Balances::free_balance(255);
//         assert_eq!(initial_balance, final_balance);
//     });
// }
//
// #[test]
// fn distributor_distribute() {
//     use tests_distributor::{Request, WASM_BINARY_BLOATY};
//
//     new_test_ext().execute_with(|| {
//         let balance_initial = Balances::free_balance(1) + Balances::free_balance(255);
//
//         let program_id =
//             generate_program_id(WASM_BINARY_BLOATY.expect("Wasm binary missing!"), &[]);
//
//         Pallet::<Test>::submit_program(
//             Origin::signed(1).into(),
//             WASM_BINARY_BLOATY.expect("Wasm binary missing!").to_vec(),
//             vec![],
//             vec![],
//             10_000_000_u64,
//             0_u128,
//         )
//         .expect("Submit program failed");
//
//         Pallet::<Test>::send_message(
//             Origin::signed(1).into(),
//             program_id,
//             Request::Receive(10).encode(),
//             20_000_000_u64,
//             0_u128,
//         )
//         .expect("Send message failed");
//
//         run_to_block(3, None);
//
//         let final_balance = Balances::free_balance(1) + Balances::free_balance(255);
//
//         assert_eq!(balance_initial, final_balance);
//     });
// }
//
// #[test]
// fn test_code_submission_pass() {
//     let wat = r#"
//     (module
//     )"#;
//
//     init_logger();
//     new_test_ext().execute_with(|| {
//         let code = parse_wat(wat);
//         let code_hash = compute_code_hash(&code);
//
//         assert_ok!(Pallet::<Test>::submit_code(Origin::signed(1), code.clone()));
//
//         let saved_code = common::get_code(code_hash);
//         assert_eq!(saved_code, Some(code));
//
//         let expected_meta = Some(CodeMetadata::new(1.into_origin(), 1));
//         let actual_meta = common::get_code_metadata(code_hash);
//         assert_eq!(expected_meta, actual_meta);
//
//         System::assert_last_event(crate::Event::CodeSaved(code_hash).into());
//     })
// }
//
// #[test]
// fn test_same_code_submission_fails() {
//     let wat = r#"
//     (module
//     )"#;
//
//     init_logger();
//     new_test_ext().execute_with(|| {
//         let code = parse_wat(wat);
//
//         assert_ok!(Pallet::<Test>::submit_code(Origin::signed(1), code.clone()),);
//         // Trying to set the same code twice.
//         assert_noop!(
//             Pallet::<Test>::submit_code(Origin::signed(1), code.clone()),
//             Error::<Test>::CodeAlreadyExists,
//         );
//         // Trying the same from another origin
//         assert_noop!(
//             Pallet::<Test>::submit_code(Origin::signed(3), code.clone()),
//             Error::<Test>::CodeAlreadyExists,
//         );
//     })
// }
//
// #[test]
// fn test_code_is_not_submitted_twice_after_program_submission() {
//     let wat = r#"
//     (module
//     )"#;
//
//     init_logger();
//     new_test_ext().execute_with(|| {
//         let code = parse_wat(wat);
//         let code_hash = compute_code_hash(&code);
//
//         // First submit program, which will set code and metadata
//         assert_ok!(Pallet::<Test>::submit_program(
//             Origin::signed(3).into(),
//             code.clone(),
//             b"salt".to_vec(),
//             Vec::new(),
//             10_000u64,
//             0_u128
//         ));
//         System::assert_has_event(crate::Event::CodeSaved(code_hash).into());
//         assert!(common::code_exists(code_hash));
//
//         // Trying to set the same code twice.
//         assert_noop!(
//             Pallet::<Test>::submit_code(Origin::signed(3), code),
//             Error::<Test>::CodeAlreadyExists,
//         );
//     })
// }
//
// #[test]
// fn test_code_is_not_resetted_within_program_submission() {
//     let wat = r#"
//     (module
//     )"#;
//
//     init_logger();
//     new_test_ext().execute_with(|| {
//         let code = parse_wat(wat);
//         let code_hash = compute_code_hash(&code);
//
//         // First submit code
//         assert_ok!(Pallet::<Test>::submit_code(Origin::signed(1), code.clone()));
//         let expected_code_saved_events = 1;
//         let expected_meta = common::get_code_metadata(code_hash);
//         assert!(expected_meta.is_some());
//
//         // Submit program from another origin. Should not change meta or code.
//         assert_ok!(Pallet::<Test>::submit_program(
//             Origin::signed(3).into(),
//             code.clone(),
//             b"salt".to_vec(),
//             Vec::new(),
//             10_000u64,
//             0_u128
//         ));
//         let actual_meta = common::get_code_metadata(code_hash);
//         let actual_code_saved_events = System::events()
//             .iter()
//             .filter(|e| matches!(e.event, mock::Event::Gear(pallet::Event::CodeSaved(_))))
//             .count();
//
//         assert_eq!(expected_meta, actual_meta);
//         assert_eq!(expected_code_saved_events, actual_code_saved_events);
//     })
// }
