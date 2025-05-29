// This file is part of Gear.

// Copyright (C) 2021-2025 Gear Technologies Inc.
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
use sp_runtime::traits::{One, Zero};
use utils::{DEFAULT_BALANCE, DEFAULT_VALIDITY};

#[test]
fn voucher_issue_works() {
    new_test_ext().execute_with(|| {
        let program_id = H256::random().cast();

        let initial_balance = Balances::free_balance(ALICE);

        let voucher_id =
            utils::issue_w_balance_and_uploading(ALICE, BOB, DEFAULT_BALANCE, program_id, false)
                .expect("Failed to issue voucher");

        assert_eq!(
            initial_balance,
            Balances::free_balance(voucher_id.cast::<AccountIdOf<Test>>())
                + Balances::free_balance(ALICE)
        );

        let voucher_id_2 =
            utils::issue_w_balance_and_uploading(ALICE, BOB, DEFAULT_BALANCE, program_id, true)
                .expect("Failed to issue voucher");

        assert_ne!(voucher_id, voucher_id_2);

        let voucher_info = Vouchers::<Test>::get(BOB, voucher_id).expect("Voucher isn't found");
        assert_eq!(voucher_info.owner, ALICE);
        assert_eq!(voucher_info.programs, Some([program_id].into()));
        assert_eq!(
            voucher_info.expiry,
            System::block_number().saturating_add(DEFAULT_VALIDITY + 1)
        );
        assert!(!voucher_info.code_uploading);

        let voucher_info = Vouchers::<Test>::get(BOB, voucher_id_2).expect("Voucher isn't found");
        assert!(voucher_info.code_uploading);
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
            Voucher::issue(
                RuntimeOrigin::signed(ALICE),
                BOB,
                1_000,
                Some(set),
                false,
                100
            ),
            Error::<Test>::MaxProgramsLimitExceeded,
        );

        // Not enough balance.
        assert_noop!(
            utils::issue_w_balance_and_uploading(ALICE, BOB, 1_000_000_000_000, program_id, false),
            Error::<Test>::BalanceTransfer
        );

        // Out of bounds validity.
        let checker = |duration: BlockNumber| {
            assert_noop!(
                Voucher::issue(
                    RuntimeOrigin::signed(ALICE),
                    BOB,
                    1_000,
                    Some([program_id].into()),
                    false,
                    duration,
                ),
                Error::<Test>::DurationOutOfBounds,
            );
        };

        checker(Zero::zero());
        checker(MinVoucherDuration::get().saturating_sub(One::one()));
        checker(MaxVoucherDuration::get().saturating_add(One::one()));
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

        // Checking case of any program.
        assert_ok!(Voucher::issue(
            RuntimeOrigin::signed(ALICE),
            BOB,
            1_000,
            None,
            false,
            DEFAULT_VALIDITY,
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

        // Checking case of code uploading.
        assert_ok!(Voucher::issue(
            RuntimeOrigin::signed(ALICE),
            BOB,
            1_000,
            Some(Default::default()),
            true,
            DEFAULT_VALIDITY,
        ));
        let voucher_id_code = utils::get_last_voucher_id();

        // For current mock PrepaidCallDispatcher set as (), so this call passes
        // successfully, but in real runtime the result will be
        // `Err(pallet_gear::Error::CodeConstructionFailed)`.
        assert_ok!(Voucher::call(
            RuntimeOrigin::signed(BOB),
            voucher_id_code,
            PrepaidCall::UploadCode { code: vec![] },
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

        // Voucher doesn't allow code uploading.
        assert_noop!(
            Voucher::call(
                RuntimeOrigin::signed(BOB),
                voucher_id,
                PrepaidCall::UploadCode { code: vec![] },
            ),
            Error::<Test>::CodeUploadingDisabled
        );

        // Voucher is out of date.
        System::set_block_number(System::block_number() + DEFAULT_VALIDITY + 1);

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
    })
}

#[test]
fn voucher_revoke_works() {
    new_test_ext().execute_with(|| {
        let program_id = H256::random().cast();

        let voucher_id = utils::issue(ALICE, BOB, program_id).expect("Failed to issue voucher");
        let voucher_id_acc = voucher_id.cast::<AccountIdOf<Test>>();

        System::set_block_number(System::block_number() + DEFAULT_VALIDITY + 1);

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
        let duration_prolong = 10;
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
            // forbid code uploading (already forbidden)
            Some(false),
            // prolong duration
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
            // allow code uploading
            Some(true),
            // prolong duration
            Some(duration_prolong),
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
        assert!(voucher.code_uploading);
        assert_eq!(
            voucher.expiry,
            System::block_number() + DEFAULT_VALIDITY + 1 + duration_prolong
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
            // code uploading
            None,
            // prolong duration
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
            voucher.expiry,
            System::block_number() + DEFAULT_VALIDITY + 1 + duration_prolong
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
            // code uploading
            Some(true),
            // prolong duration
            None,
        )));

        let huge_block = 10_000_000_000;
        let duration_prolong = 10;
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
            // code uploading
            None,
            // prolong duration
            Some(duration_prolong),
        ));

        let voucher = Vouchers::<Test>::get(BOB, voucher_id).expect("Failed to get voucher");
        assert_eq!(voucher.expiry, huge_block + 1 + duration_prolong);

        let valid_prolong = MaxVoucherDuration::get() - (voucher.expiry - huge_block);

        // -1 due to voucher was prolonged as expired.
        assert_eq!(
            valid_prolong,
            MaxVoucherDuration::get() - duration_prolong - 1
        );

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
                // code uploading
                None,
                // prolong duration
                Some(valid_prolong + 1),
            ),
            Error::<Test>::DurationOutOfBounds
        );

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
            // code uploading
            None,
            // prolong duration
            Some(valid_prolong),
        ),);
    });
}

#[test]
fn voucher_update_err_cases() {
    new_test_ext().execute_with(|| {
        let program_id = H256::random().cast();

        let voucher_id =
            utils::issue_w_balance_and_uploading(ALICE, BOB, DEFAULT_BALANCE, program_id, true)
                .expect("Failed to issue voucher");

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
                // code uploading
                None,
                // prolong duration
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
                // code uploading
                None,
                // prolong duration
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
                // code uploading
                None,
                // prolong duration
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
                // code uploading
                None,
                // prolong duration
                None,
            ),
            Error::<Test>::MaxProgramsLimitExceeded
        );

        // Try to restrict code uploading.
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
                None,
                // code uploading
                Some(false),
                // prolong duration
                None,
            ),
            Error::<Test>::CodeUploadingEnabled
        );

        // Prolongation duration error.
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
                None,
                // code uploading
                None,
                // prolong duration
                Some(MaxVoucherDuration::get().saturating_sub(DEFAULT_VALIDITY)),
            ),
            Error::<Test>::DurationOutOfBounds
        );
    });
}

#[test]
fn voucher_decline_works() {
    new_test_ext().execute_with(|| {
        assert_ok!(Voucher::issue(
            RuntimeOrigin::signed(ALICE),
            BOB,
            DEFAULT_BALANCE,
            None,
            false,
            DEFAULT_VALIDITY,
        ));

        let voucher_id = utils::get_last_voucher_id();

        assert_ok!(Voucher::decline(RuntimeOrigin::signed(BOB), voucher_id));

        System::assert_last_event(
            Event::VoucherDeclined {
                spender: BOB,
                voucher_id,
            }
            .into(),
        );

        let new_expiry = Vouchers::<Test>::get(BOB, voucher_id)
            .expect("Couldn't find voucher")
            .expiry;

        assert_eq!(new_expiry, System::block_number());
    });
}

#[test]
fn voucher_decline_err_cases() {
    new_test_ext().execute_with(|| {
        // Voucher doesn't exist.
        assert_noop!(
            Voucher::decline(RuntimeOrigin::signed(BOB), H256::random().cast()),
            Error::<Test>::InexistentVoucher
        );

        // Voucher has already expired.
        assert_ok!(Voucher::issue(
            RuntimeOrigin::signed(ALICE),
            BOB,
            DEFAULT_BALANCE,
            None,
            false,
            DEFAULT_VALIDITY,
        ));

        let voucher_id = utils::get_last_voucher_id();

        System::set_block_number(System::block_number() + 10 * DEFAULT_VALIDITY);

        assert_noop!(
            Voucher::decline(RuntimeOrigin::signed(BOB), voucher_id),
            Error::<Test>::VoucherExpired
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
    pub(crate) fn issue(
        from: AccountIdOf<Test>,
        to: AccountIdOf<Test>,
        program: ActorId,
    ) -> Result<VoucherId, DispatchErrorWithPostInfo> {
        issue_w_balance_and_uploading(from, to, DEFAULT_BALANCE, program, false)
    }

    #[track_caller]
    pub(crate) fn issue_w_balance_and_uploading(
        from: AccountIdOf<Test>,
        to: AccountIdOf<Test>,
        balance: BalanceOf<Test>,
        program: ActorId,
        code_uploading: bool,
    ) -> Result<VoucherId, DispatchErrorWithPostInfo> {
        Voucher::issue(
            RuntimeOrigin::signed(from),
            to,
            balance,
            Some([program].into()),
            code_uploading,
            DEFAULT_VALIDITY,
        )
        .map(|_| get_last_voucher_id())
    }

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
}
