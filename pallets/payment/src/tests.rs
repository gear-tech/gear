// This file is part of Gear.

// Copyright (C) 2021-2022 Gear Technologies Inc.
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

use crate::{mock::*, AdditionalTxValidator, Config, CustomChargeTransactionPayment, QueueOf};
use codec::Encode;
use common::{storage::*, GasPrice, Origin};
use frame_support::{
    assert_err, assert_ok,
    dispatch::{DispatchInfo, GetDispatchInfo, PostDispatchInfo},
    pallet_prelude::*,
    traits::Currency,
    weights::{Weight, WeightToFee},
};
use gear_core::{
    ids::{MessageId, ProgramId},
    message::{Dispatch, DispatchKind, Message, StoredDispatch},
};
use pallet_gear::Call;
use pallet_transaction_payment::{FeeDetails, InclusionFee, Multiplier, RuntimeDispatchInfo};
use primitive_types::H256;
use sp_runtime::{testing::TestXt, traits::SignedExtension, FixedPointNumber};
use RuntimeCall::Gear;

type WeightToFeeFor<T> = <T as pallet_transaction_payment::Config>::WeightToFee;
type LengthToFeeFor<T> = <T as pallet_transaction_payment::Config>::LengthToFee;
type AdditionalTxValidatorOf<T> = <T as Config>::AdditionalTxValidator;

const EXISTENTIAL_DEPOSIT: u64 = ExistentialDeposit::get();

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

fn init_logger() {
    let _ = env_logger::Builder::from_default_env()
        .format_module_path(false)
        .format_level(true)
        .try_init();
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
fn validation_works() {
    init_logger();
    new_test_ext().execute_with(|| {
        let bob_initial_balance = Balances::free_balance(BOB);
        let gas_limit = 100_000;
        let value = 100_000;
        let gas_value = GasConverter::gas_price(gas_limit);

        let call = Gear(Call::send_message {
            destination: ProgramId::from_origin(H256::random()),
            payload: vec![],
            gas_limit,
            value,
        });

        let tip = 0;
        let len = 100;
        let info = info_from_weight(Weight::from_ref_time(100));

        let tx_fee = TransactionPayment::compute_fee(len as u32, &info, tip);
        let fee_for_msg = value + gas_value;

        // Check validation works
        assert_ok!(Balances::transfer(
            RuntimeOrigin::signed(ALICE),
            BOB,
            tx_fee + fee_for_msg
        ));
        assert_ok!(
            CustomChargeTransactionPayment::<Test>::from(tip).validate(&BOB, &call, &info, len)
        );
        assert_ok!(<AdditionalTxValidatorOf<Test> as AdditionalTxValidator<
            Test,
        >>::validate(&BOB, &call));

        // Check `pre_dispatch` does the same
        assert_ok!(Balances::transfer(
            RuntimeOrigin::signed(ALICE),
            BOB,
            // Validation doesn't withdraw funds for gas and value
            tx_fee
        ));
        assert_ok!(
            CustomChargeTransactionPayment::<Test>::from(tip).validate(&BOB, &call, &info, len)
        );
        assert_ok!(<AdditionalTxValidatorOf<Test> as AdditionalTxValidator<
            Test,
        >>::validate(&BOB, &call));

        // Check invalidation works
        // Bob has fee_for_msg already, so transferring him tx_fee will allow to include his tx to the pool.
        // That's why we send tx_fee - 1. We also lower that amount by Bob's initial balance before these ops.
        // to be sure he will not have enough funds to include his tx to the pool.
        assert_ok!(Balances::transfer(
            RuntimeOrigin::signed(ALICE),
            BOB,
            tx_fee - 1 - bob_initial_balance
        ));
        assert_err!(
            CustomChargeTransactionPayment::<Test>::from(tip).validate(&BOB, &call, &info, len),
            TransactionValidityError::Invalid(InvalidTransaction::Payment)
        );
        // This check shows, that the previous call failed only due to `AdditionalTxValidator`, as `ChargeTransactionPayment` successfully withdrew funds.
        assert_eq!(Balances::free_balance(BOB), fee_for_msg - 1);

        // Check pre_dispatch does the same
        assert_ok!(Balances::transfer(
            RuntimeOrigin::signed(ALICE),
            BOB,
            // Validation doesn't withdraw funds for gas and value
            tx_fee
        ));
        assert_err!(
            CustomChargeTransactionPayment::<Test>::from(tip).validate(&BOB, &call, &info, len),
            TransactionValidityError::Invalid(InvalidTransaction::Payment)
        );
        // This check shows, that the previous call failed only due to `AdditionalTxValidator`, as `ChargeTransactionPayment` successfully withdrew funds.
        assert_eq!(Balances::free_balance(BOB), fee_for_msg - 1);
    })
}

#[test]
fn validation_works_when_tx_invalidated_in_pool() {
    new_test_ext().execute_with(|| {
        let bob_initial_balance = Balances::free_balance(BOB);
        let gas_limit = 100_000;
        let value = 100_000;
        let gas_value = GasConverter::gas_price(gas_limit);

        let call = Gear(Call::send_message {
            destination: ProgramId::from_origin(H256::random()),
            payload: vec![],
            gas_limit,
            value,
        });

        let tip = 0;
        let len = 100;
        let info = info_from_weight(Weight::from_ref_time(100));

        let tx_fee = TransactionPayment::compute_fee(len as u32, &info, tip);
        let fee_for_msg = value + gas_value;

        assert_ok!(Balances::transfer(
            RuntimeOrigin::signed(ALICE),
            BOB,
            tx_fee + fee_for_msg
        ));
        let bob_actual_balance = Balances::free_balance(BOB);

        // Say Bob's tx is validated
        assert_ok!(
            CustomChargeTransactionPayment::<Test>::from(tip).validate(&BOB, &call, &info, len)
        );

        // as `validate` call from runtime doesn't change the state, we return funds to BOB
        assert_ok!(Balances::transfer(
            RuntimeOrigin::signed(ALICE),
            BOB,
            // returning by ALICE's expense...
            tx_fee
        ));
        assert_eq!(Balances::free_balance(BOB), bob_actual_balance);

        // So Bob's tx is in the pool. Later it invalidates, because Bob's funds decrease.s
        assert_ok!(Balances::transfer(
            RuntimeOrigin::signed(BOB),
            ALICE,
            fee_for_msg
        ));
        // Next pre_dispatch call with fail
        assert_err!(
            CustomChargeTransactionPayment::<Test>::from(tip).validate(&BOB, &call, &info, len),
            TransactionValidityError::Invalid(InvalidTransaction::Payment)
        );
        // This check shows, that the previous call failed only due to `AdditionalTxValidator`, as `ChargeTransactionPayment` successfully withdrew funds.
        assert_eq!(Balances::free_balance(BOB), bob_initial_balance);
    })
}

// Tests that all calls without gear message data pass `AdditionalTxValidation`.
#[test]
fn validation_for_calls_without_gas_and_value() {
    new_test_ext().execute_with(|| {
        let non_message_sending_calls = [
            Gear(Call::upload_code { code: vec![] }),
            Gear(Call::claim_value {
                message_id: MessageId::from_origin(H256::random()),
            }),
            Gear(Call::reset {}),
            Gear(Call::run {}),
        ];

        let tip = 0;

        let test = |call: RuntimeCall| {
            // Absolutely random data
            let len = 100;
            let info = info_from_weight(Weight::from_ref_time(100));

            let required_for_fee = TransactionPayment::compute_fee(len as u32, &info, tip);

            // Give minimum balance to the sender
            assert_ok!(Balances::transfer(
                RuntimeOrigin::signed(ALICE),
                BOB,
                required_for_fee
            ));

            // Check Tx is valid
            assert_ok!(
                CustomChargeTransactionPayment::<Test>::from(tip).validate(&BOB, &call, &info, len)
            );

            // Even remove account
            assert_ok!(Balances::transfer(
                RuntimeOrigin::signed(BOB),
                100,
                Balances::total_balance(&BOB)
            ));

            // Dummy - do more external check
            assert_ok!(<AdditionalTxValidatorOf<Test> as AdditionalTxValidator<
                Test,
            >>::validate(&BOB, &call));

            // Give existential deposit back
            assert_ok!(Balances::set_balance(
                RuntimeOrigin::root(),
                BOB,
                EXISTENTIAL_DEPOSIT as u128,
                0
            ));
        };

        // Run test
        for call in non_message_sending_calls {
            test(call);
        }
    })
}

#[test]
fn custom_fee_multiplier_updated_per_block() {
    new_test_ext().execute_with(|| {
        // Send n extrinsics and run to next block
        populate_message_queue::<Test>(10);
        run_to_block(2);

        // CustomFeeMultiplier is 2^(10 / 5) == 4
        assert_eq!(
            TransactionPayment::next_fee_multiplier(),
            Multiplier::saturating_from_integer(4)
        );

        populate_message_queue::<Test>(33);
        run_to_block(3);

        // CustomFeeMultiplier is 2^(33 / 5) == 64
        assert_eq!(
            TransactionPayment::next_fee_multiplier(),
            Multiplier::saturating_from_integer(64)
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

            let rounding_error = WeightToFeeFor::<Test>::weight_to_fee(&Weight::from_ref_time(mul));

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
            Weight::from_ref_time(1_000),
            Weight::from_ref_time(100_000),
            Weight::from_ref_time(10_000_000),
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
            });

        let len = 100usize;
        let fee_length = LengthToFeeFor::<Test>::weight_to_fee(&Weight::from_ref_time(len as u64));

        let weight = Weight::from_ref_time(1_000);

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

        // Fee multiplier should have been set to 16
        let pre = CustomChargeTransactionPayment::<Test>::from(0)
            .pre_dispatch(&ALICE, call, &info_from_weight(weight), len)
            .unwrap();

        assert_eq!(
            Balances::free_balance(ALICE),
            alice_initial_balance - (fee_weight * 16 + fee_length)
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
            alice_initial_balance - (fee_weight * 16 + fee_length)
        );
        assert_eq!(
            Balances::free_balance(BLOCK_AUTHOR),
            author_initial_balance + (fee_weight * 16 + fee_length)
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
        let fee_length = LengthToFeeFor::<Test>::weight_to_fee(&Weight::from_ref_time(len as u64));

        let weight = Weight::from_ref_time(1_000);

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

        let rounding_error = WeightToFeeFor::<Test>::weight_to_fee(&Weight::from_ref_time(16));
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
        let fee_affecting_length = LengthToFeeFor::<Test>::weight_to_fee(&Weight::from_ref_time(len_affecting_mq.into()));
        assert_eq!(
            GearPayment::query_info::<_, GasConverter>(xt_affecting_mq.clone(), len_affecting_mq),
            RuntimeDispatchInfo {
                weight: info_affecting_mq.weight,
                class: info_affecting_mq.class,
                partial_fee: 0 /* base_fee */
                    + fee_affecting_length  /* len * 1 */
                    + fee_affecting_weight /* weight */
            },
        );

        let fee_weight = WeightToFeeFor::<Test>::weight_to_fee(&info_not_affecting_mq.weight);
        let fee_length = LengthToFeeFor::<Test>::weight_to_fee(&Weight::from_ref_time(len_not_affecting_mq.into()));
        assert_eq!(
            GearPayment::query_info::<_, GasConverter>(xt_not_affecting_mq.clone(), len_not_affecting_mq),
            RuntimeDispatchInfo {
                weight: info_not_affecting_mq.weight,
                class: info_not_affecting_mq.class,
                partial_fee: 0 /* base_fee */
                    + fee_length  /* len * 1 */
                    + fee_weight /* weight */
            },
        );

        assert_eq!(
            GearPayment::query_info::<_, GasConverter>(unsigned_xt.clone(), len_affecting_mq),
            RuntimeDispatchInfo {
                weight: unsigned_xt_info.weight,
                class: unsigned_xt_info.class,
                partial_fee: 0,
            },
        );

        assert_eq!(
            GearPayment::query_fee_details::<_, GasConverter>(xt_affecting_mq.clone(), len_affecting_mq),
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
            GearPayment::query_fee_details::<_, GasConverter>(xt_not_affecting_mq.clone(), len_not_affecting_mq),
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
            GearPayment::query_fee_details::<_, GasConverter>(unsigned_xt.clone(), len_affecting_mq),
            FeeDetails {
                inclusion_fee: None,
                tip: 0
            },
        );

        // Now populate message queue
        populate_message_queue::<Test>(20);
        run_to_block(2);

        // Extra fee multiplier is now 2^(20 / 5) == 16
        assert_eq!(
            GearPayment::query_info::<_, GasConverter>(xt_affecting_mq.clone(), len_affecting_mq),
            RuntimeDispatchInfo {
                weight: info_affecting_mq.weight,
                class: info_affecting_mq.class,
                partial_fee: 0 /* base_fee */
                    + fee_affecting_length  /* len * 1 */
                    + fee_affecting_weight * 16u128 /* weight * 16 */
            },
        );

        // Extra fee not applicable => fee should be exactly what it was for empty MQ
        // However, we must account for the rounding error in this case
        let rounding_error = WeightToFeeFor::<Test>::weight_to_fee(&Weight::from_ref_time(16));
        assert_eq!(
            GearPayment::query_info::<_, GasConverter>(xt_not_affecting_mq.clone(), len_not_affecting_mq),
            RuntimeDispatchInfo {
                weight: info_not_affecting_mq.weight,
                class: info_not_affecting_mq.class,
                partial_fee: 0 /* base_fee */
                    + fee_length  /* len * 1 */
                    + (fee_weight / rounding_error) * rounding_error /* weight, with potential small rounding error */
            },
        );

        assert_eq!(
            GearPayment::query_info::<_, GasConverter>(unsigned_xt.clone(), len_affecting_mq),
            RuntimeDispatchInfo {
                weight: unsigned_xt_info.weight,
                class: unsigned_xt_info.class,
                partial_fee: 0,
            },
        );

        assert_eq!(
            GearPayment::query_fee_details::<_, GasConverter>(xt_affecting_mq, len_affecting_mq),
            FeeDetails {
                inclusion_fee: Some(InclusionFee {
                    base_fee: 0,
                    len_fee: fee_affecting_length,
                    adjusted_weight_fee: fee_affecting_weight * 16u128,
                }),
                tip: 0,
            },
        );

        assert_eq!(
            GearPayment::query_fee_details::<_, GasConverter>(xt_not_affecting_mq, len_not_affecting_mq),
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
            GearPayment::query_fee_details::<_, GasConverter>(unsigned_xt, len_affecting_mq),
            FeeDetails {
                inclusion_fee: None,
                tip: 0
            },
        );
    });
}
