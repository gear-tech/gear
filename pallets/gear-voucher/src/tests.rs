// This file is part of Gear.

// Copyright (C) 2021-2023 Gear Technologies Inc.
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

use super::*;
use crate::mock::*;
use common::Origin;
use frame_support::{assert_noop, assert_ok, assert_storage_noop};
use primitive_types::H256;
use sp_runtime::traits::Zero;

#[test]
fn voucher_issue_works() {
    new_test_ext().execute_with(|| {
        let program_id = H256::random().cast();

        let initial_balance = Balances::free_balance(ALICE);

        let voucher_id = utils::issue(ALICE, BOB, program_id).expect("Failed to issue voucher");

        assert_eq!(
            initial_balance,
            Balances::free_balance(voucher_id.cast::<AccountIdOf<Test>>())
                + Balances::free_balance(ALICE)
        );

        let voucher_id_2 = utils::issue(ALICE, BOB, program_id).expect("Failed to issue voucher");

        assert_ne!(voucher_id, voucher_id_2);

        let voucher_info = Vouchers::<Test>::get(BOB, voucher_id).expect("Voucher isn't found");
        assert_eq!(voucher_info.owner, ALICE);
        assert_eq!(voucher_info.programs, Some([program_id].into()));
        assert_eq!(
            voucher_info.validity,
            System::block_number().saturating_add(utils::DEFAULT_VALIDITY)
        );
    });
}

#[test]
fn voucher_issue_err_cases() {
    new_test_ext().execute_with(|| {
        let program_id = H256::random().cast();

        // Programs limit.
        let set = (0..=<<Test as Config>::MaxProgramsAmount as Get<u8>>::get())
            .map(|_| H256::random().cast())
            .collect();

        assert_noop!(
            Voucher::issue(RuntimeOrigin::signed(ALICE), BOB, 1_000, Some(set), 100),
            Error::<Test>::MaxProgramsLimitExceeded,
        );

        // Not enough balance.
        assert_noop!(
            utils::issue_w_balance(ALICE, BOB, 1_000_000_000_000, program_id),
            Error::<Test>::BalanceTransfer
        );

        // Zero validity.
        assert_noop!(
            Voucher::issue(
                RuntimeOrigin::signed(ALICE),
                BOB,
                1_000,
                Some([program_id].into()),
                0,
            ),
            Error::<Test>::ZeroValidity,
        );
    });
}

#[test]
fn voucher_call_works() {
    new_test_ext().execute_with(|| {
        let program_id = MAILBOXED_PROGRAM;

        let voucher_id = utils::issue(ALICE, BOB, program_id).expect("Failed to issue voucher");

        assert_ok!(Voucher::call(
            RuntimeOrigin::signed(BOB),
            voucher_id,
            PrepaidCall::SendMessage {
                destination: program_id,
                payload: vec![],
                gas_limit: 0,
                value: 0,
                keep_alive: false
            }
        ));

        assert_ok!(Voucher::call(
            RuntimeOrigin::signed(BOB),
            voucher_id,
            PrepaidCall::SendReply {
                reply_to_id: MAILBOXED_MESSAGE,
                payload: vec![],
                gas_limit: 0,
                value: 0,
                keep_alive: false
            },
        ));

        // Always ok, because legacy call doesn't access vouchers storage
        // and just proxies payment to specific synthetic account.
        assert_ok!(Voucher::call_deprecated(
            RuntimeOrigin::signed(ALICE),
            PrepaidCall::SendMessage {
                destination: H256::random().cast(),
                payload: vec![],
                gas_limit: 0,
                value: 0,
                keep_alive: false
            }
        ));

        // Ok if message exists in mailbox.
        assert_ok!(Voucher::call_deprecated(
            RuntimeOrigin::signed(ALICE),
            PrepaidCall::SendReply {
                reply_to_id: MAILBOXED_MESSAGE,
                payload: vec![],
                gas_limit: 0,
                value: 0,
                keep_alive: false
            },
        ));

        // Checking case of any program.
        assert_ok!(Voucher::issue(
            RuntimeOrigin::signed(ALICE),
            BOB,
            1_000,
            None,
            utils::DEFAULT_VALIDITY,
        ));
        let voucher_id_any = utils::get_last_voucher_id();

        assert_ok!(Voucher::call(
            RuntimeOrigin::signed(BOB),
            voucher_id_any,
            PrepaidCall::SendMessage {
                destination: program_id,
                payload: vec![],
                gas_limit: 0,
                value: 0,
                keep_alive: false
            }
        ));

        assert_ok!(Voucher::call(
            RuntimeOrigin::signed(BOB),
            voucher_id_any,
            PrepaidCall::SendReply {
                reply_to_id: MAILBOXED_MESSAGE,
                payload: vec![],
                gas_limit: 0,
                value: 0,
                keep_alive: false
            },
        ));
    })
}

#[test]
fn voucher_call_err_cases() {
    new_test_ext().execute_with(|| {
        let program_id = H256::random().cast();

        // Voucher doesn't exist at all.
        assert_noop!(
            Voucher::call(
                RuntimeOrigin::signed(BOB),
                H256::random().cast(),
                PrepaidCall::SendMessage {
                    destination: program_id,
                    payload: vec![],
                    gas_limit: 0,
                    value: 0,
                    keep_alive: false
                }
            ),
            Error::<Test>::InexistentVoucher
        );

        let voucher_id = utils::issue(ALICE, BOB, program_id).expect("Failed to issue voucher");

        // Voucher doesn't exist for the user.
        assert_noop!(
            Voucher::call(
                RuntimeOrigin::signed(ALICE),
                voucher_id,
                PrepaidCall::SendMessage {
                    destination: program_id,
                    payload: vec![],
                    gas_limit: 0,
                    value: 0,
                    keep_alive: false
                }
            ),
            Error::<Test>::InexistentVoucher
        );

        // Couldn't find destination for some prepaid calls.
        assert_noop!(
            Voucher::call(
                RuntimeOrigin::signed(BOB),
                voucher_id,
                PrepaidCall::SendReply {
                    reply_to_id: H256::random().cast(),
                    payload: vec![],
                    gas_limit: 0,
                    value: 0,
                    keep_alive: false
                },
            ),
            Error::<Test>::UnknownDestination
        );

        // Destination wasn't whitelisted.
        assert_noop!(
            Voucher::call(
                RuntimeOrigin::signed(BOB),
                voucher_id,
                PrepaidCall::SendMessage {
                    destination: H256::random().cast(),
                    payload: vec![],
                    gas_limit: 0,
                    value: 0,
                    keep_alive: false
                }
            ),
            Error::<Test>::InappropriateDestination
        );

        assert_noop!(
            Voucher::call(
                RuntimeOrigin::signed(BOB),
                voucher_id,
                PrepaidCall::SendReply {
                    reply_to_id: MAILBOXED_MESSAGE,
                    payload: vec![],
                    gas_limit: 0,
                    value: 0,
                    keep_alive: false
                },
            ),
            Error::<Test>::InappropriateDestination
        );

        // Voucher is out of date.
        System::set_block_number(System::block_number() + utils::DEFAULT_VALIDITY + 1);

        assert_noop!(
            Voucher::call(
                RuntimeOrigin::signed(BOB),
                voucher_id,
                PrepaidCall::SendMessage {
                    destination: program_id,
                    payload: vec![],
                    gas_limit: 0,
                    value: 0,
                    keep_alive: false
                }
            ),
            Error::<Test>::VoucherExpired
        );

        // Message doesn't exist in mailbox.
        assert_noop!(
            Voucher::call_deprecated(
                RuntimeOrigin::signed(BOB),
                PrepaidCall::SendReply {
                    reply_to_id: H256::random().cast(),
                    payload: vec![],
                    gas_limit: 0,
                    value: 0,
                    keep_alive: false
                },
            ),
            Error::<Test>::UnknownDestination
        );
    })
}

#[test]
fn voucher_revoke_works() {
    new_test_ext().execute_with(|| {
        let program_id = H256::random().cast();

        let voucher_id = utils::issue(ALICE, BOB, program_id).expect("Failed to issue voucher");
        let voucher_id_acc = voucher_id.cast::<AccountIdOf<Test>>();

        System::set_block_number(System::block_number() + utils::DEFAULT_VALIDITY + 1);

        let balance_after_issue = Balances::free_balance(ALICE);
        let voucher_balance = Balances::free_balance(voucher_id_acc);

        assert!(!voucher_balance.is_zero());

        assert_ok!(Voucher::revoke(
            RuntimeOrigin::signed(ALICE),
            BOB,
            voucher_id
        ));

        System::assert_has_event(
            crate::Event::VoucherRevoked {
                spender: BOB,
                voucher_id,
            }
            .into(),
        );

        // NOTE: To be changed to `assert_ne!` once `revoke()` deletes voucher.
        assert!(Vouchers::<Test>::get(BOB, voucher_id).is_some());
        assert!(Balances::free_balance(voucher_id_acc).is_zero());
        assert_eq!(
            Balances::free_balance(ALICE),
            balance_after_issue + voucher_balance
        );

        assert_storage_noop!(assert_ok!(Voucher::revoke(
            RuntimeOrigin::signed(ALICE),
            BOB,
            voucher_id
        )));
    })
}

#[test]
fn voucher_revoke_err_cases() {
    new_test_ext().execute_with(|| {
        let program_id = H256::random().cast();

        let voucher_id = utils::issue(ALICE, BOB, program_id).expect("Failed to issue voucher");

        // Voucher doesn't exist
        assert_noop!(
            Voucher::revoke(RuntimeOrigin::signed(ALICE), BOB, H256::random().cast(),),
            Error::<Test>::InexistentVoucher
        );

        assert_noop!(
            Voucher::revoke(RuntimeOrigin::signed(ALICE), ALICE, voucher_id,),
            Error::<Test>::InexistentVoucher
        );

        // NOTE: To be changed once `revoke()` could be called by non-owner.
        // Non-owner revoke.
        assert_noop!(
            Voucher::revoke(RuntimeOrigin::signed(BOB), BOB, voucher_id,),
            Error::<Test>::BadOrigin
        );

        // Voucher is not expired yet.
        assert_noop!(
            Voucher::revoke(RuntimeOrigin::signed(ALICE), BOB, voucher_id,),
            Error::<Test>::IrrevocableYet
        );
    });
}

#[test]
fn voucher_update_works() {
    new_test_ext().execute_with(|| {
        let program_id = H256::random().cast();

        let voucher_id = utils::issue(ALICE, BOB, program_id).expect("Failed to issue voucher");
        let voucher_id_acc = voucher_id.cast::<AccountIdOf<Test>>();

        let alice_balance = Balances::free_balance(ALICE);
        let voucher_balance = Balances::free_balance(voucher_id_acc);

        let new_program_id = H256::random().cast();
        let validity_prolong = 10;
        let balance_top_up = 1_000;

        assert_storage_noop!(assert_ok!(Voucher::update(
            RuntimeOrigin::signed(ALICE),
            BOB,
            voucher_id,
            // move ownership
            Some(ALICE),
            // balance top up
            Some(0),
            // extra programs
            Some(Some([program_id].into())),
            // prolong validity
            Some(0),
        )));

        assert_ok!(Voucher::update(
            RuntimeOrigin::signed(ALICE),
            BOB,
            voucher_id,
            // move ownership
            Some(BOB),
            // balance top up
            Some(balance_top_up),
            // extra programs
            Some(Some([new_program_id].into())),
            // prolong validity
            Some(validity_prolong),
        ));

        System::assert_has_event(
            crate::Event::VoucherUpdated {
                spender: BOB,
                voucher_id,
                new_owner: Some(BOB),
            }
            .into(),
        );

        let voucher = Vouchers::<Test>::get(BOB, voucher_id).expect("Failed to get voucher");

        assert_eq!(
            Balances::free_balance(ALICE),
            alice_balance - balance_top_up
        );
        assert_eq!(
            Balances::free_balance(voucher_id_acc),
            voucher_balance + balance_top_up
        );
        assert_eq!(voucher.owner, BOB);
        assert_eq!(voucher.programs, Some([program_id, new_program_id].into()));
        assert_eq!(
            voucher.validity,
            System::block_number() + utils::DEFAULT_VALIDITY + validity_prolong
        );

        let voucher_balance = Balances::free_balance(voucher_id_acc);

        assert_ok!(Voucher::update(
            RuntimeOrigin::signed(BOB),
            BOB,
            voucher_id,
            // move ownership
            None,
            // balance top up
            None,
            // extra programs
            Some(None),
            // prolong validity
            None,
        ));

        System::assert_has_event(
            crate::Event::VoucherUpdated {
                spender: BOB,
                voucher_id,
                new_owner: None,
            }
            .into(),
        );

        let voucher = Vouchers::<Test>::get(BOB, voucher_id).expect("Failed to get voucher");

        assert_eq!(Balances::free_balance(voucher_id_acc), voucher_balance);
        assert_eq!(voucher.owner, BOB);
        assert_eq!(voucher.programs, None);
        assert_eq!(
            voucher.validity,
            System::block_number() + utils::DEFAULT_VALIDITY + validity_prolong
        );

        assert_storage_noop!(assert_ok!(Voucher::update(
            RuntimeOrigin::signed(BOB),
            BOB,
            voucher_id,
            // move ownership
            None,
            // balance top up
            None,
            // extra programs
            Some(Some([program_id].into())),
            // prolong validity
            None,
        )));

        let huge_block = 10_000_000_000;
        let validity_prolong = 10;
        System::set_block_number(huge_block);

        assert_ok!(Voucher::update(
            RuntimeOrigin::signed(BOB),
            BOB,
            voucher_id,
            // move ownership
            None,
            // balance top up
            None,
            // extra programs
            None,
            // prolong validity
            Some(validity_prolong),
        ));

        let voucher = Vouchers::<Test>::get(BOB, voucher_id).expect("Failed to get voucher");
        assert_eq!(voucher.validity, huge_block + validity_prolong);
    });
}

#[test]
fn voucher_update_err_cases() {
    new_test_ext().execute_with(|| {
        let program_id = H256::random().cast();

        let voucher_id = utils::issue(ALICE, BOB, program_id).expect("Failed to issue voucher");

        // Inexistent voucher.
        assert_noop!(
            Voucher::update(
                RuntimeOrigin::signed(ALICE),
                ALICE,
                voucher_id,
                // move ownership
                None,
                // balance top up
                None,
                // extra programs
                None,
                // prolong validity
                None,
            ),
            Error::<Test>::InexistentVoucher
        );

        // Update by non-owner.
        assert_noop!(
            Voucher::update(
                RuntimeOrigin::signed(BOB),
                BOB,
                voucher_id,
                // move ownership
                None,
                // balance top up
                None,
                // extra programs
                None,
                // prolong validity
                None,
            ),
            Error::<Test>::BadOrigin
        );

        // Balances error.
        assert_noop!(
            Voucher::update(
                RuntimeOrigin::signed(ALICE),
                BOB,
                voucher_id,
                // move ownership
                None,
                // balance top up
                Some(100_000_000_000_000),
                // extra programs
                None,
                // prolong validity
                None,
            ),
            Error::<Test>::BalanceTransfer
        );

        // Programs limit exceed.
        let set = (0..=<<Test as Config>::MaxProgramsAmount as Get<u8>>::get())
            .map(|_| H256::random().cast())
            .collect();

        assert_noop!(
            Voucher::update(
                RuntimeOrigin::signed(ALICE),
                BOB,
                voucher_id,
                // move ownership
                None,
                // balance top up
                None,
                // extra programs
                Some(Some(set)),
                // prolong validity
                None,
            ),
            Error::<Test>::MaxProgramsLimitExceeded
        );
    });
}

mod utils {
    use super::*;
    use frame_support::dispatch::DispatchErrorWithPostInfo;
    use frame_system::pallet_prelude::BlockNumberFor;

    pub(crate) const DEFAULT_VALIDITY: BlockNumberFor<Test> = 100;
    pub(crate) const DEFAULT_BALANCE: BalanceOf<Test> = ExistentialDeposit::get() * 1_000;

    #[track_caller]
    pub(crate) fn get_last_voucher_id() -> VoucherId {
        System::events()
            .iter()
            .rev()
            .filter_map(|r| {
                if let crate::mock::RuntimeEvent::Voucher(e) = r.event.clone() {
                    Some(e)
                } else {
                    None
                }
            })
            .find_map(|e| match e {
                crate::Event::VoucherIssued { voucher_id, .. } => Some(voucher_id),
                _ => None,
            })
            .expect("can't find voucher issued event")
    }

    #[track_caller]
    pub(crate) fn issue_w_balance(
        from: AccountIdOf<Test>,
        to: AccountIdOf<Test>,
        balance: BalanceOf<Test>,
        program: ProgramId,
    ) -> Result<VoucherId, DispatchErrorWithPostInfo> {
        Voucher::issue(
            RuntimeOrigin::signed(from),
            to,
            balance,
            Some([program].into()),
            DEFAULT_VALIDITY,
        )
        .map(|_| get_last_voucher_id())
    }

    #[track_caller]
    pub(crate) fn issue(
        from: AccountIdOf<Test>,
        to: AccountIdOf<Test>,
        program: ProgramId,
    ) -> Result<VoucherId, DispatchErrorWithPostInfo> {
        issue_w_balance(from, to, DEFAULT_BALANCE, program)
    }
}
