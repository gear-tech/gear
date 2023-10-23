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

#![allow(clippy::identity_op)]

use crate::{mock::*, Config, CustomChargeTransactionPayment, QueueOf};
use common::{storage::*, Origin};
use frame_support::{
    assert_ok,
    codec::Encode,
    dispatch::{DispatchInfo, GetDispatchInfo, PostDispatchInfo},
    weights::{Weight, WeightToFee},
};
use gear_core::{
    ids::{MessageId, ProgramId},
    message::{Dispatch, DispatchKind, Message, StoredDispatch, UserStoredMessage},
};
use pallet_transaction_payment::{FeeDetails, InclusionFee, Multiplier, RuntimeDispatchInfo};
use primitive_types::H256;
use sp_runtime::{testing::TestXt, traits::SignedExtension, FixedPointNumber};

type WeightToFeeFor<T> = <T as pallet_transaction_payment::Config>::WeightToFee;
type LengthToFeeFor<T> = <T as pallet_transaction_payment::Config>::LengthToFee;

macro_rules! assert_approx_eq {
    ($left:expr, $right:expr, $tol:expr) => {{
        assert!(
            $left < $right + $tol && $right < $left + $tol,
            "{} != {} with tolerance {}",
            $left,
            $right,
            $tol
        );
    }};
}

fn info_from_weight(weight: Weight) -> DispatchInfo {
    // DispatchInfo { weight: w, class: DispatchClass::Normal, pays_fee: Pays::Yes }
    DispatchInfo {
        weight,
        ..Default::default()
    }
}

fn default_post_info() -> PostDispatchInfo {
    PostDispatchInfo {
        actual_weight: None,
        ..Default::default()
    }
}

fn populate_message_queue<T>(n: u64)
where
    T: Config,
    T::Messenger: Messenger<QueuedDispatch = StoredDispatch>,
{
    QueueOf::<T>::clear();

    for i in 0_u64..n {
        let prog_id = (i + 1).into();
        let msg_id = (100_u64 * n + i + 1).into();
        let user_id = (10_000_u64 * n + i + 1).into();
        let gas_limit = Some(10_000_u64);
        let dispatch = Dispatch::new(
            DispatchKind::Handle,
            Message::new(
                msg_id,
                user_id,
                prog_id,
                Default::default(),
                gas_limit,
                0,
                None,
            ),
        );

        let dispatch = dispatch.into_stored();

        assert_ok!(QueueOf::<T>::queue(dispatch).map_err(|_| "Error pushing back stored dispatch"));
    }
}

#[test]
fn custom_fee_multiplier_updated_per_block() {
    new_test_ext().execute_with(|| {
        // Send n extrinsics and run to next block
        populate_message_queue::<Test>(10);
        run_to_block(2);

        // CustomFeeMultiplier is (10 / 5 + 1) == 3
        assert_eq!(
            TransactionPayment::next_fee_multiplier(),
            Multiplier::saturating_from_integer(3)
        );

        populate_message_queue::<Test>(33);
        run_to_block(3);

        // CustomFeeMultiplier is (33 / 5 + 1) == 7
        assert_eq!(
            TransactionPayment::next_fee_multiplier(),
            Multiplier::saturating_from_integer(7)
        );
    });
}

#[test]
fn fee_rounding_error_bounded_by_multiplier() {
    new_test_ext().execute_with(|| {
        // Test various combinations:
        // - large weight, small multiplier
        // - large weight, large (relatively) multiplier
        // - relatively small weight, small multiplier
        // - relatively small weight, relatively large multiplier

        let test_case = |call: &<Test as frame_system::Config>::RuntimeCall,
                         weights: Vec<Weight>,
                         mul: u64| {
            // not charging for tx len to make rounding error more significant
            let len = 0;

            let rounding_error = WeightToFeeFor::<Test>::weight_to_fee(&Weight::from_parts(mul, 0));

            for w in weights {
                let alice_initial_balance = Balances::free_balance(ALICE);
                let author_initial_balance = Balances::free_balance(BLOCK_AUTHOR);

                let pre = CustomChargeTransactionPayment::<Test>::from(0)
                    .pre_dispatch(&ALICE, call, &info_from_weight(w), len)
                    .unwrap();

                let fee = WeightToFeeFor::<Test>::weight_to_fee(&w);
                assert_approx_eq!(
                    Balances::free_balance(ALICE),
                    alice_initial_balance - fee,
                    rounding_error
                );

                assert_ok!(CustomChargeTransactionPayment::<Test>::post_dispatch(
                    Some(pre),
                    &info_from_weight(w),
                    &default_post_info(),
                    len,
                    &Ok(())
                ));
                assert_approx_eq!(
                    Balances::free_balance(BLOCK_AUTHOR),
                    author_initial_balance + fee,
                    rounding_error
                );
            }
        };

        // rounding error only arises for calls that do not affect MQ
        let call: &<Test as frame_system::Config>::RuntimeCall =
            &RuntimeCall::Gear(pallet_gear::Call::claim_value {
                message_id: MessageId::from_origin(H256::from_low_u64_le(1)),
            });

        let weights = vec![
            Weight::from_parts(1_000, 0),
            Weight::from_parts(100_000, 0),
            Weight::from_parts(10_000_000, 0),
        ];

        // MQ is empty => multiplier is 1. No rounding error expected
        test_case(call, weights.clone(), 1);

        // Now populate message queue with 20 => multiplier == 16
        populate_message_queue::<Test>(20);
        run_to_block(2);
        test_case(call, weights.clone(), 16);

        // Populate message queue with 60 => multiplier == 4096
        populate_message_queue::<Test>(60);
        run_to_block(3);
        test_case(call, weights, 4096);
    });
}

#[test]
fn mq_size_affecting_fee_works() {
    new_test_ext().execute_with(|| {
        // Scenario:
        //
        // - clear dispatch queue
        // - submit transaction of known weight and len that affects MQ
        // - ensure the fee is "standard": `len_fee` + `unadjusted_weight_fee`

        // Populate MQ
        // In the next block re-submit the transaction from before and check that
        // - the fee factors in an additional custom multiplier that affects weight_fee part,
        // - balances add up.

        let alice_initial_balance = Balances::free_balance(ALICE);
        let author_initial_balance = Balances::free_balance(BLOCK_AUTHOR);

        let program_id = ProgramId::from_origin(H256::random());

        let call: &<Test as frame_system::Config>::RuntimeCall =
            &RuntimeCall::Gear(pallet_gear::Call::send_message {
                destination: program_id,
                payload: Default::default(),
                gas_limit: 100_000,
                value: 0,
                keep_alive: false,
            });

        let len = 100usize;
        let fee_length = LengthToFeeFor::<Test>::weight_to_fee(&Weight::from_parts(len as u64, 0));

        let weight = Weight::from_parts(1_000, 0);

        let pre = CustomChargeTransactionPayment::<Test>::from(0)
            .pre_dispatch(&ALICE, call, &info_from_weight(weight), len)
            .unwrap();

        let fee_weight = WeightToFeeFor::<Test>::weight_to_fee(&weight);
        // Can use strict equality for calls that do not introduce rounding error
        assert_eq!(
            Balances::free_balance(ALICE),
            alice_initial_balance - fee_weight - fee_length
        );

        assert_ok!(CustomChargeTransactionPayment::<Test>::post_dispatch(
            Some(pre),
            &info_from_weight(weight),
            &default_post_info(),
            len,
            &Ok(())
        ));
        assert_eq!(
            Balances::free_balance(ALICE),
            alice_initial_balance - fee_weight - fee_length
        );
        assert_eq!(
            Balances::free_balance(BLOCK_AUTHOR),
            author_initial_balance + fee_weight + fee_length
        );

        // Now populate message queue
        populate_message_queue::<Test>(20);

        run_to_block(2);

        let alice_initial_balance = Balances::free_balance(ALICE);
        let author_initial_balance = Balances::free_balance(BLOCK_AUTHOR);

        // Fee multiplier should have been set to 5
        let pre = CustomChargeTransactionPayment::<Test>::from(0)
            .pre_dispatch(&ALICE, call, &info_from_weight(weight), len)
            .unwrap();

        assert_eq!(
            Balances::free_balance(ALICE),
            alice_initial_balance - (fee_weight * 5 + fee_length)
        );

        assert_ok!(CustomChargeTransactionPayment::<Test>::post_dispatch(
            Some(pre),
            &info_from_weight(weight),
            &default_post_info(),
            len,
            &Ok(())
        ));
        assert_eq!(
            Balances::free_balance(ALICE),
            alice_initial_balance - (fee_weight * 5 + fee_length)
        );
        assert_eq!(
            Balances::free_balance(BLOCK_AUTHOR),
            author_initial_balance + (fee_weight * 5 + fee_length)
        );
    });
}

#[test]
fn mq_size_not_affecting_fee_works() {
    new_test_ext().execute_with(|| {
        // Scenario:
        //
        // - clear dispatch queue
        // - submit transaction of known weight and len that does not affect MQ
        // - ensure the fee is "standard": `len_fee` + `unadjusted_weight_fee`

        // Populate MQ
        // In the next block re-submit the transaction from before and check that
        // - the fee remains unchanged,
        // - balances add up.

        let alice_initial_balance = Balances::free_balance(ALICE);
        let author_initial_balance = Balances::free_balance(BLOCK_AUTHOR);

        let call: &<Test as frame_system::Config>::RuntimeCall =
            &RuntimeCall::Gear(pallet_gear::Call::claim_value {
                message_id: MessageId::from_origin(H256::from_low_u64_le(1)),
            });

        let len = 100usize;
        let fee_length = LengthToFeeFor::<Test>::weight_to_fee(&Weight::from_parts(len as u64, 0));

        let weight = Weight::from_parts(1_000, 0);

        let pre = CustomChargeTransactionPayment::<Test>::from(0)
            .pre_dispatch(&ALICE, call, &info_from_weight(weight), len)
            .unwrap();

        let fee_weight = WeightToFeeFor::<Test>::weight_to_fee(&weight);
        assert_approx_eq!(
            Balances::free_balance(ALICE),
            alice_initial_balance - fee_weight - fee_length,
            1
        );

        assert_ok!(CustomChargeTransactionPayment::<Test>::post_dispatch(
            Some(pre),
            &info_from_weight(weight),
            &default_post_info(),
            len,
            &Ok(())
        ));
        assert_eq!(
            Balances::free_balance(ALICE),
            alice_initial_balance - fee_weight - fee_length
        );
        assert_approx_eq!(
            Balances::free_balance(ALICE),
            alice_initial_balance - fee_weight - fee_length,
            1
        );
        assert_approx_eq!(
            Balances::free_balance(BLOCK_AUTHOR),
            author_initial_balance + fee_weight + fee_length,
            1
        );

        // Now populate message queue
        populate_message_queue::<Test>(20);

        run_to_block(2);

        let alice_initial_balance = Balances::free_balance(ALICE);
        let author_initial_balance = Balances::free_balance(BLOCK_AUTHOR);

        // Fee multiplier should have been set to 16
        let pre = CustomChargeTransactionPayment::<Test>::from(0)
            .pre_dispatch(&ALICE, call, &info_from_weight(weight), len)
            .unwrap();

        let rounding_error = WeightToFeeFor::<Test>::weight_to_fee(&Weight::from_parts(16, 0));
        // Now we may have some rounding error somewhere at the least significant digits
        assert_approx_eq!(
            Balances::free_balance(ALICE),
            alice_initial_balance - fee_weight - fee_length,
            rounding_error
        );

        assert_ok!(CustomChargeTransactionPayment::<Test>::post_dispatch(
            Some(pre),
            &info_from_weight(weight),
            &default_post_info(),
            len,
            &Ok(())
        ));
        assert_approx_eq!(
            Balances::free_balance(ALICE),
            alice_initial_balance - fee_weight - fee_length,
            rounding_error
        );
        assert_approx_eq!(
            Balances::free_balance(BLOCK_AUTHOR),
            author_initial_balance + fee_weight + fee_length,
            rounding_error
        );
    });
}

#[test]
#[allow(clippy::let_unit_value)]
fn query_info_and_fee_details_work() {
    let program_id = ProgramId::from_origin(H256::random());
    let call_affecting_mq = RuntimeCall::Gear(pallet_gear::Call::send_message {
        destination: program_id,
        payload: Default::default(),
        gas_limit: 100_000,
        value: 0,
        keep_alive: false,
    });
    let call_not_affecting_mq = RuntimeCall::Gear(pallet_gear::Call::claim_value {
        message_id: 1.into(),
    });
    let extra = ();

    let xt_affecting_mq = TestXt::new(call_affecting_mq.clone(), Some((ALICE, extra)));
    let info_affecting_mq = xt_affecting_mq.get_dispatch_info();
    let ext_affecting_mq = xt_affecting_mq.encode();
    let len_affecting_mq = ext_affecting_mq.len() as u32;

    let xt_not_affecting_mq = TestXt::new(call_not_affecting_mq, Some((ALICE, extra)));
    let info_not_affecting_mq = xt_not_affecting_mq.get_dispatch_info();
    let ext_not_affecting_mq = xt_not_affecting_mq.encode();
    let len_not_affecting_mq = ext_not_affecting_mq.len() as u32;

    let unsigned_xt = TestXt::<_, ()>::new(call_affecting_mq, None);
    let unsigned_xt_info = unsigned_xt.get_dispatch_info();

    new_test_ext().execute_with(|| {
        // Empty Message queue => extra fee is not applied
        let fee_affecting_weight = WeightToFeeFor::<Test>::weight_to_fee(&info_affecting_mq.weight);
        let fee_affecting_length = LengthToFeeFor::<Test>::weight_to_fee(&Weight::from_parts(len_affecting_mq.into(), 0));
        assert_eq!(
            GearPayment::query_info(xt_affecting_mq.clone(), len_affecting_mq),
            RuntimeDispatchInfo {
                weight: info_affecting_mq.weight,
                class: info_affecting_mq.class,
                partial_fee: 0 /* base_fee */
                    + fee_affecting_length  /* len * 1 */
                    + fee_affecting_weight /* weight */
            },
        );

        let fee_weight = WeightToFeeFor::<Test>::weight_to_fee(&info_not_affecting_mq.weight);
        let fee_length = LengthToFeeFor::<Test>::weight_to_fee(&Weight::from_parts(len_not_affecting_mq.into(), 0));
        assert_eq!(
            GearPayment::query_info(xt_not_affecting_mq.clone(), len_not_affecting_mq),
            RuntimeDispatchInfo {
                weight: info_not_affecting_mq.weight,
                class: info_not_affecting_mq.class,
                partial_fee: 0 /* base_fee */
                    + fee_length  /* len * 1 */
                    + fee_weight /* weight */
            },
        );

        assert_eq!(
            GearPayment::query_info(unsigned_xt.clone(), len_affecting_mq),
            RuntimeDispatchInfo {
                weight: unsigned_xt_info.weight,
                class: unsigned_xt_info.class,
                partial_fee: 0,
            },
        );

        assert_eq!(
            GearPayment::query_fee_details(xt_affecting_mq.clone(), len_affecting_mq),
            FeeDetails {
                inclusion_fee: Some(InclusionFee {
                    base_fee: 0,
                    len_fee: fee_affecting_length,
                    adjusted_weight_fee: fee_affecting_weight,
                }),
                tip: 0,
            },
        );

        assert_eq!(
            GearPayment::query_fee_details(xt_not_affecting_mq.clone(), len_not_affecting_mq),
            FeeDetails {
                inclusion_fee: Some(InclusionFee {
                    base_fee: 0,
                    len_fee: fee_length,
                    adjusted_weight_fee: fee_weight,
                }),
                tip: 0,
            },
        );

        assert_eq!(
            GearPayment::query_fee_details(unsigned_xt.clone(), len_affecting_mq),
            FeeDetails {
                inclusion_fee: None,
                tip: 0
            },
        );

        // Now populate message queue
        populate_message_queue::<Test>(20);
        run_to_block(2);

        // Extra fee multiplier is now (20 / 5 + 1) == 5
        assert_eq!(
            GearPayment::query_info(xt_affecting_mq.clone(), len_affecting_mq),
            RuntimeDispatchInfo {
                weight: info_affecting_mq.weight,
                class: info_affecting_mq.class,
                partial_fee: 0 /* base_fee */
                    + fee_affecting_length  /* len * 1 */
                    + fee_affecting_weight * 5u128 /* weight * 5 */
            },
        );

        // Extra fee not applicable => fee should be exactly what it was for empty MQ
        // However, we must account for the rounding error in this case
        let rounding_error = WeightToFeeFor::<Test>::weight_to_fee(&Weight::from_parts(5, 0));
        assert_eq!(
            GearPayment::query_info(xt_not_affecting_mq.clone(), len_not_affecting_mq),
            RuntimeDispatchInfo {
                weight: info_not_affecting_mq.weight,
                class: info_not_affecting_mq.class,
                partial_fee: 0 /* base_fee */
                    + fee_length  /* len * 1 */
                    + (fee_weight / rounding_error) * rounding_error /* weight, with potential small rounding error */
            },
        );

        assert_eq!(
            GearPayment::query_info(unsigned_xt.clone(), len_affecting_mq),
            RuntimeDispatchInfo {
                weight: unsigned_xt_info.weight,
                class: unsigned_xt_info.class,
                partial_fee: 0,
            },
        );

        assert_eq!(
            GearPayment::query_fee_details(xt_affecting_mq, len_affecting_mq),
            FeeDetails {
                inclusion_fee: Some(InclusionFee {
                    base_fee: 0,
                    len_fee: fee_affecting_length,
                    adjusted_weight_fee: fee_affecting_weight * 5u128,
                }),
                tip: 0,
            },
        );

        assert_eq!(
            GearPayment::query_fee_details(xt_not_affecting_mq, len_not_affecting_mq),
            FeeDetails {
                inclusion_fee: Some(InclusionFee {
                    base_fee: 0,
                    len_fee: fee_length,
                    adjusted_weight_fee: (fee_weight / rounding_error) * rounding_error,
                }),
                tip: 0,
            },
        );

        assert_eq!(
            GearPayment::query_fee_details(unsigned_xt, len_affecting_mq),
            FeeDetails {
                inclusion_fee: None,
                tip: 0
            },
        );
    });
}

#[test]
fn fee_payer_replacement_works() {
    new_test_ext().execute_with(|| {
        let alice_initial_balance = Balances::free_balance(ALICE);
        let author_initial_balance = Balances::free_balance(BLOCK_AUTHOR);
        let synthesized_initial_balance = Balances::free_balance(FEE_PAYER);

        let program_id = ProgramId::from_origin(H256::random());

        let call: &<Test as frame_system::Config>::RuntimeCall =
            &RuntimeCall::GearVoucher(pallet_gear_voucher::Call::call {
                call: pallet_gear_voucher::PrepaidCall::SendMessage {
                    destination: program_id,
                    payload: Default::default(),
                    gas_limit: 100_000,
                    value: 0,
                    keep_alive: false,
                },
            });

        let len = 100usize;
        let fee_length = LengthToFeeFor::<Test>::weight_to_fee(&Weight::from_parts(len as u64, 0));

        let weight = Weight::from_parts(1_000, 0);

        let pre = CustomChargeTransactionPayment::<Test>::from(0)
            .pre_dispatch(&ALICE, call, &info_from_weight(weight), len)
            .unwrap();

        let fee_weight = WeightToFeeFor::<Test>::weight_to_fee(&weight);

        // Alice hasn't paid fees
        assert_eq!(Balances::free_balance(ALICE), alice_initial_balance);

        // But the Synthesized account has
        assert_eq!(
            Balances::free_balance(FEE_PAYER),
            synthesized_initial_balance - fee_weight - fee_length
        );

        assert_ok!(CustomChargeTransactionPayment::<Test>::post_dispatch(
            Some(pre),
            &info_from_weight(weight),
            &default_post_info(),
            len,
            &Ok(())
        ));
        assert_eq!(Balances::free_balance(ALICE), alice_initial_balance);
        assert_eq!(
            Balances::free_balance(FEE_PAYER),
            synthesized_initial_balance - fee_weight - fee_length
        );
        assert_eq!(
            Balances::free_balance(BLOCK_AUTHOR),
            author_initial_balance + fee_weight + fee_length
        );
    });
}

#[test]
fn reply_with_voucher_pays_fee_from_voucher_ok() {
    new_test_ext().execute_with(|| {
        let alice_initial_balance = Balances::free_balance(ALICE);
        let author_initial_balance = Balances::free_balance(BLOCK_AUTHOR);
        let bob_initial_balance = Balances::free_balance(BOB);

        let msg_id = MessageId::from_origin(H256::random());
        let program_id = ProgramId::from_origin(H256::random());
        // Put message in BOB's mailbox
        assert_ok!(MailboxOf::<Test>::insert(
            UserStoredMessage::new(
                msg_id,
                program_id,
                ProgramId::from_origin(BOB.into_origin()),
                Default::default(),
                Default::default(),
            ),
            5_u64
        ));

        // ALICE issues a voucher to BOB
        assert_ok!(GearVoucher::issue(
            RuntimeOrigin::signed(ALICE),
            BOB,
            program_id,
            200_000_000,
        ));
        let voucher_id = GearVoucher::voucher_account_id(&BOB, &program_id);

        run_to_block(2);

        // Balance check
        assert_eq!(Balances::free_balance(voucher_id), 200_000_000);
        assert_eq!(
            Balances::free_balance(ALICE),
            alice_initial_balance.saturating_sub(200_000_000)
        );

        // Preparing a call
        let gas_limit = 100_000_u64;
        let call: &<Test as frame_system::Config>::RuntimeCall =
            &RuntimeCall::GearVoucher(pallet_gear_voucher::Call::call {
                call: pallet_gear_voucher::PrepaidCall::SendReply {
                    reply_to_id: msg_id,
                    payload: vec![],
                    gas_limit,
                    value: 0,
                    keep_alive: false,
                },
            });

        let len = 100_usize;
        let fee_length = LengthToFeeFor::<Test>::weight_to_fee(&Weight::from_parts(len as u64, 0));

        let weight = Weight::from_parts(100_000, 0);

        let voucher_initial_balance = Balances::free_balance(voucher_id);

        let pre = CustomChargeTransactionPayment::<Test>::from(0)
            .pre_dispatch(&BOB, call, &info_from_weight(weight), len)
            .unwrap();

        let fee_weight = WeightToFeeFor::<Test>::weight_to_fee(&weight);

        // BOB hasn't paid fees
        assert_eq!(Balances::free_balance(BOB), bob_initial_balance);

        // But the voucher account has
        assert_eq!(
            Balances::free_balance(voucher_id),
            voucher_initial_balance - fee_weight - fee_length
        );

        assert_ok!(CustomChargeTransactionPayment::<Test>::post_dispatch(
            Some(pre),
            &info_from_weight(weight),
            &default_post_info(),
            len,
            &Ok(())
        ));

        // Block autho has got his cut
        assert_eq!(
            Balances::free_balance(BLOCK_AUTHOR),
            author_initial_balance + fee_weight + fee_length
        );
    })
}
