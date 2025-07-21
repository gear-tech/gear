// This file is part of Gear.

// Copyright (C) 2024-2025 Gear Technologies Inc.
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

use crate::mock::*;
use ark_bls12_381::{Bls12_381, G1Affine, G1Projective as G1, G2Affine, G2Projective as G2};
use ark_ec::{
    Group, ScalarMul, VariableBaseMSM,
    bls12::Bls12Config,
    hashing::{HashToCurve, curve_maps::wb, map_to_curve_hasher::MapToCurveBasedHasher},
    pairing::Pairing,
    short_weierstrass::{Projective as SWProjective, SWCurveConfig},
};
use ark_ff::{biginteger::BigInt, fields::field_hashers::DefaultFieldHasher};
use ark_scale::hazmat::ArkScaleProjective;
use ark_std::{UniformRand, ops::Mul};
use common::Origin;
use frame_support::assert_ok;
use gbuiltin_bls381::*;
use gear_core::ids::ActorId;
use gear_core_errors::{ErrorReplyReason, ReplyCode, SimpleExecutionError};
use gear_runtime_interface::DST_G2;
use pallet_gear::GasInfo;
use parity_scale_codec::{Decode, Encode};
use primitive_types::H256;

type ArkScale<T> = ark_scale::ArkScale<T, { ark_scale::HOST_CALL }>;
type ScalarField = <G2 as Group>::ScalarField;
type WBMap = wb::WBMap<<ark_bls12_381::Config as Bls12Config>::G2Config>;

const ACTOR_ID: [u8; 32] =
    hex_literal::hex!("6b6e292c382945e80bf51af2ba7fe9f458dcff81ae6075c46f9095e1bbecdc37");

pub(crate) fn init_logger() {
    let _ = tracing_subscriber::fmt::try_init();
}

fn get_gas_info(builtin_id: ActorId, payload: Vec<u8>) -> GasInfo {
    start_transaction();
    let res = Gear::calculate_gas_info(
        SIGNER.into_origin(),
        pallet_gear::manager::HandleKind::Handle(builtin_id),
        payload,
        0,
        true,
        None,
        None,
    )
    .expect("calculate_gas_info failed");
    rollback_transaction();

    assert_ne!(res.min_limit, 0);
    assert_ne!(res.burned, 0);
    // < 90% * block_gas_limit
    assert!(res.burned < BlockGasLimit::get() / 10 * 9);

    res
}

#[test]
fn decoding_error() {
    init_logger();

    new_test_ext().execute_with(|| {
        let builtin_actor_id: ActorId = H256::from(ACTOR_ID).cast();

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(SIGNER),
            builtin_actor_id,
            vec![255u8; 10],
            1_000_000_000,
            0,
            false,
        ));

        run_to_next_block();

        // An error reply should have been sent.
        assert!(System::events().into_iter().any(|e| match e.event {
            RuntimeEvent::Gear(pallet_gear::Event::<Test>::UserMessageSent { message, .. }) => {
                message.destination() == SIGNER.cast()
                    && matches!(message.details(), Some(details) if details.to_reply_code()
                    == ReplyCode::Error(ErrorReplyReason::Execution(
                        SimpleExecutionError::UserspacePanic,
                    )))
            }
            _ => false,
        }));
    });
}

#[test]
fn multi_miller_loop() {
    init_logger();

    new_test_ext().execute_with(|| {
        let mut rng = ark_std::test_rng();

        let message: G1Affine = G1::rand(&mut rng).into();
        let priv_key: ScalarField = UniformRand::rand(&mut rng);
        let generator: G2 = G2::generator();
        let pub_key: G2Affine = generator.mul(priv_key).into();

        let a: ArkScale<Vec<<Bls12_381 as Pairing>::G1Affine>> = vec![message].into();
        let b: ArkScale<Vec<<Bls12_381 as Pairing>::G2Affine>> = vec![].into();
        let payload = Request::MultiMillerLoop { a: a.encode(), b: b.encode(), }.encode();

        // Case of the incorrect arguments
        let builtin_id: ActorId = H256::from(ACTOR_ID).cast();
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(SIGNER),
            builtin_id,
            payload.clone(),
            10_000_000_000,
            0,
            false,
        ));

        run_to_next_block();

        assert!(System::events().into_iter().any(|e| match e.event {
            RuntimeEvent::Gear(pallet_gear::Event::<Test>::UserMessageSent { message, .. }) => {
                message.destination() == SIGNER.cast()
                    && matches!(message.details(), Some(details) if details.to_reply_code()
                    == ReplyCode::Error(ErrorReplyReason::Execution(
                        SimpleExecutionError::UserspacePanic,
                    )))
            }
            _ => false,
        }));

        let result = <Bls12_381 as Pairing>::multi_miller_loop(vec![message], vec![pub_key]);

        let a: ArkScale<Vec<<Bls12_381 as Pairing>::G1Affine>> = vec![message].into();
        let b: ArkScale<Vec<<Bls12_381 as Pairing>::G2Affine>> = vec![pub_key].into();
        let payload = Request::MultiMillerLoop { a: a.encode(), b: b.encode(), }.encode();

        let builtin_id: ActorId = H256::from(ACTOR_ID).cast();
        let gas_info = get_gas_info(builtin_id, payload.clone());

        // Check the case of insufficient gas
        System::reset_events();

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(SIGNER),
            builtin_id,
            payload.clone(),
            gas_info.min_limit / 2,
            0,
            false,
        ));

        run_to_next_block();

        // An error reply should have been sent.
        assert!(System::events().into_iter().any(|e| match e.event {
            RuntimeEvent::Gear(pallet_gear::Event::<Test>::UserMessageSent { message, .. }) => {
                message.destination() == SIGNER.cast()
                    && matches!(message.details(), Some(details) if details.to_reply_code()
                    == ReplyCode::Error(ErrorReplyReason::Execution(
                        SimpleExecutionError::RanOutOfGas,
                    )))
            }
            _ => false,
        }));

        // Check the case of the block gas allowance having been exceeded
        System::reset_events();

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(SIGNER),
            builtin_id,
            payload.clone(),
            gas_info.min_limit,
            0,
            false,
        ));

        run_for_n_blocks(1, Some(gas_info.min_limit - 1));

        // The dispatch is still in the queue
        assert!(!message_queue_empty());

        // Check the computations are correct
        System::reset_events();

        // No need to send another message, the dispatch is still in the queue
        run_to_next_block();

        let response = match System::events().into_iter().find_map(|e| match e.event {
            RuntimeEvent::Gear(pallet_gear::Event::<Test>::UserMessageSent { message, .. }) => {
                assert_eq!(message.destination(), SIGNER.cast());
                assert!(matches!(message.details(), Some(details) if matches!(details.to_reply_code(), ReplyCode::Success(..))));

                Some(message.payload_bytes().to_vec())
            }

            _ => None,
        }) {
            Some(response) => response,
            _ => unreachable!(),
        };

        let builtin_result = match Response::decode(&mut response.as_slice()) {
            Ok(Response::MultiMillerLoop(builtin_result)) => builtin_result,
            _ => unreachable!(),
        };

        let builtin_result = ArkScale::<<Bls12_381 as Pairing>::TargetField>::decode(&mut builtin_result.as_slice()).unwrap();
        assert_eq!(result.0, builtin_result.0);
    });
}

#[test]
fn final_exponentiation() {
    init_logger();

    new_test_ext().execute_with(|| {
        let mut rng = ark_std::test_rng();

        // message
        let message: G1Affine = G1::rand(&mut rng).into();
        let priv_key: ScalarField = UniformRand::rand(&mut rng);
        let generator: G2 = G2::generator();
        let pub_key: G2Affine = generator.mul(priv_key).into();

        let loop_result = <Bls12_381 as Pairing>::multi_miller_loop(vec![message], vec![pub_key]);
        let result = <Bls12_381 as Pairing>::final_exponentiation(loop_result);

        let f: ArkScale<<Bls12_381 as Pairing>::TargetField> = loop_result.0.into();
        let payload = Request::FinalExponentiation { f: f.encode() }.encode();

        let builtin_actor_id: ActorId = H256::from(ACTOR_ID).cast();
        let gas_info = get_gas_info(builtin_actor_id, payload.clone());

        // check case of insufficient gas
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(SIGNER),
            builtin_actor_id,
            payload.clone(),
            gas_info.min_limit / 2,
            0,
            false,
        ));

        run_to_next_block();

        // An error reply should have been sent.
        assert!(System::events().into_iter().any(|e| match e.event {
            RuntimeEvent::Gear(pallet_gear::Event::<Test>::UserMessageSent { message, .. }) => {
                message.destination() == SIGNER.cast()
                    && matches!(message.details(), Some(details) if details.to_reply_code()
                    == ReplyCode::Error(ErrorReplyReason::Execution(
                        SimpleExecutionError::RanOutOfGas,
                    )))
            }
            _ => false,
        }));

        // check the computations are correct
        System::reset_events();

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(SIGNER),
            builtin_actor_id,
            payload,
            gas_info.min_limit,
            0,
            false,
        ));

        run_to_next_block();

        let response = match System::events().into_iter().find_map(|e| match e.event {
            RuntimeEvent::Gear(pallet_gear::Event::<Test>::UserMessageSent { message, .. }) => {
                assert_eq!(message.destination(), SIGNER.cast());
                assert!(matches!(message.details(), Some(details) if matches!(details.to_reply_code(), ReplyCode::Success(..))));

                Some(message.payload_bytes().to_vec())
            }

            _ => None,
        }) {
            Some(response) => response,
            _ => unreachable!(),
        };

        let builtin_result = match Response::decode(&mut response.as_slice()) {
            Ok(Response::FinalExponentiation(builtin_result)) => builtin_result,
            _ => unreachable!(),
        };

        let builtin_result = ArkScale::<<Bls12_381 as Pairing>::TargetField>::decode(&mut builtin_result.as_slice()).unwrap();
        assert!(matches!(result, Some(r) if r.0 == builtin_result.0));
    });
}

#[test]
fn msm_g1() {
    init_logger();

    new_test_ext().execute_with(|| {
        let mut rng = ark_std::test_rng();

        let count = 5usize;

        let scalars = (0..count)
            .map(|_| <G1 as Group>::ScalarField::rand(&mut rng))
            .collect::<Vec<_>>();

        let bases = (0..count).map(|_| G1::rand(&mut rng)).collect::<Vec<_>>();
        let bases = G1::batch_convert_to_mul_base(&bases);

        let ark_scalars: ArkScale<Vec<<G1 as Group>::ScalarField>> = scalars[1..].to_vec().into();
        let ark_bases: ArkScale<Vec<G1Affine>> = bases.clone().into();

        let payload = Request::MultiScalarMultiplicationG1 { bases: ark_bases.encode(), scalars: ark_scalars.encode() }.encode();

        // Case of the incorrect arguments
        let builtin_id: ActorId = H256::from(ACTOR_ID).cast();
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(SIGNER),
            builtin_id,
            payload.clone(),
            10_000_000_000,
            0,
            false,
        ));

        run_to_next_block();

        assert!(System::events().into_iter().any(|e| match e.event {
            RuntimeEvent::Gear(pallet_gear::Event::<Test>::UserMessageSent { message, .. }) => {
                message.destination() == SIGNER.cast()
                    && matches!(message.details(), Some(details) if details.to_reply_code()
                    == ReplyCode::Error(ErrorReplyReason::Execution(
                        SimpleExecutionError::UserspacePanic,
                    )))
            }
            _ => false,
        }));

        let result = <SWProjective<ark_bls12_381::g1::Config> as VariableBaseMSM>::msm(&bases, &scalars);

        let ark_scalars: ArkScale<Vec<<G1 as Group>::ScalarField>> = scalars.into();
        let ark_bases: ArkScale<Vec<G1Affine>> = bases.into();

        let payload = Request::MultiScalarMultiplicationG1 { bases: ark_bases.encode(), scalars: ark_scalars.encode() }.encode();

        let builtin_actor_id: ActorId = H256::from(ACTOR_ID).cast();
        let gas_info = get_gas_info(builtin_actor_id, payload.clone());

        // Check the case of insufficient gas
        System::reset_events();
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(SIGNER),
            builtin_actor_id,
            payload.clone(),
            gas_info.min_limit / 2,
            0,
            false,
        ));

        run_to_next_block();

        // An error reply should have been sent.
        assert!(System::events().into_iter().any(|e| match e.event {
            RuntimeEvent::Gear(pallet_gear::Event::<Test>::UserMessageSent { message, .. }) => {
                message.destination() == SIGNER.cast()
                    && matches!(message.details(), Some(details) if details.to_reply_code()
                    == ReplyCode::Error(ErrorReplyReason::Execution(
                        SimpleExecutionError::RanOutOfGas,
                    )))
            }
            _ => false,
        }));

        // Check the computations are correct
        System::reset_events();

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(SIGNER),
            builtin_actor_id,
            payload,
            gas_info.min_limit,
            0,
            false,
        ));

        run_to_next_block();

        let response = match System::events().into_iter().find_map(|e| match e.event {
            RuntimeEvent::Gear(pallet_gear::Event::<Test>::UserMessageSent { message, .. }) => {
                assert_eq!(message.destination(), SIGNER.cast());
                assert!(matches!(message.details(), Some(details) if matches!(details.to_reply_code(), ReplyCode::Success(..))));

                Some(message.payload_bytes().to_vec())
            }

            _ => None,
        }) {
            Some(response) => response,
            _ => unreachable!(),
        };

        let builtin_result = match Response::decode(&mut response.as_slice()) {
            Ok(Response::MultiScalarMultiplicationG1(builtin_result)) => builtin_result,
            _ => unreachable!(),
        };

        let builtin_result = ArkScaleProjective::<G1>::decode(&mut builtin_result.as_slice()).unwrap();
        assert!(matches!(result, Ok(r) if r == builtin_result.0));
    });
}

#[test]
fn msm_g2() {
    init_logger();

    new_test_ext().execute_with(|| {
        let mut rng = ark_std::test_rng();

        let count = 5usize;

        let scalars = (0..count)
            .map(|_| <G2 as Group>::ScalarField::rand(&mut rng))
            .collect::<Vec<_>>();

        let bases = (0..count).map(|_| G2::rand(&mut rng)).collect::<Vec<_>>();
        let bases = G2::batch_convert_to_mul_base(&bases);

        let ark_scalars: ArkScale<Vec<<G2 as Group>::ScalarField>> = scalars[1..].to_vec().into();
        let ark_bases: ArkScale<Vec<G2Affine>> = bases.clone().into();

        let payload = Request::MultiScalarMultiplicationG1 { bases: ark_bases.encode(), scalars: ark_scalars.encode() }.encode();

        // Case of the incorrect arguments
        let builtin_id: ActorId = H256::from(ACTOR_ID).cast();
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(SIGNER),
            builtin_id,
            payload.clone(),
            10_000_000_000,
            0,
            false,
        ));

        run_to_next_block();

        assert!(System::events().into_iter().any(|e| match e.event {
            RuntimeEvent::Gear(pallet_gear::Event::<Test>::UserMessageSent { message, .. }) => {
                message.destination() == SIGNER.cast()
                    && matches!(message.details(), Some(details) if details.to_reply_code()
                    == ReplyCode::Error(ErrorReplyReason::Execution(
                        SimpleExecutionError::UserspacePanic,
                    )))
            }
            _ => false,
        }));

        let result = <SWProjective<ark_bls12_381::g2::Config> as VariableBaseMSM>::msm(&bases, &scalars);

        let ark_scalars: ArkScale<Vec<<G2 as Group>::ScalarField>> = scalars.into();
        let ark_bases: ArkScale<Vec<G2Affine>> = bases.into();

        let payload = Request::MultiScalarMultiplicationG2 { bases: ark_bases.encode(), scalars: ark_scalars.encode() }.encode();

        let builtin_actor_id: ActorId = H256::from(ACTOR_ID).cast();
        let gas_info = get_gas_info(builtin_actor_id, payload.clone());

        // Check the case of insufficient gas
        System::reset_events();
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(SIGNER),
            builtin_actor_id,
            payload.clone(),
            gas_info.min_limit / 2,
            0,
            false,
        ));

        run_to_next_block();

        // An error reply should have been sent.
        assert!(System::events().into_iter().any(|e| match e.event {
            RuntimeEvent::Gear(pallet_gear::Event::<Test>::UserMessageSent { message, .. }) => {
                message.destination() == SIGNER.cast()
                    && matches!(message.details(), Some(details) if details.to_reply_code()
                    == ReplyCode::Error(ErrorReplyReason::Execution(
                        SimpleExecutionError::RanOutOfGas,
                    )))
            }
            _ => false,
        }));

        // Check the computations are correct
        System::reset_events();

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(SIGNER),
            builtin_actor_id,
            payload,
            gas_info.min_limit,
            0,
            false,
        ));

        run_to_next_block();

        let response = match System::events().into_iter().find_map(|e| match e.event {
            RuntimeEvent::Gear(pallet_gear::Event::<Test>::UserMessageSent { message, .. }) => {
                assert_eq!(message.destination(), SIGNER.cast());
                assert!(matches!(message.details(), Some(details) if matches!(details.to_reply_code(), ReplyCode::Success(..))));

                Some(message.payload_bytes().to_vec())
            }

            _ => None,
        }) {
            Some(response) => response,
            _ => unreachable!(),
        };

        let builtin_result = match Response::decode(&mut response.as_slice()) {
            Ok(Response::MultiScalarMultiplicationG2(builtin_result)) => builtin_result,
            _ => unreachable!(),
        };

        let builtin_result = ArkScaleProjective::<G2>::decode(&mut builtin_result.as_slice()).unwrap();
        assert!(matches!(result, Ok(r) if r == builtin_result.0));
    });
}

#[test]
fn mul_projective_g1() {
    init_logger();

    new_test_ext().execute_with(|| {
        let mut rng = ark_std::test_rng();

        let bigint = BigInt::<3>::rand(&mut rng);
        let bigint = bigint.0.to_vec();
        let base = G1::rand(&mut rng);

        let result = <ark_bls12_381::g1::Config as SWCurveConfig>::mul_projective(&base, &bigint);

        let ark_bigint: ArkScale<Vec<u64>> = bigint.into();
        let ark_base: ArkScaleProjective<G1> = base.into();
        let payload = Request::ProjectiveMultiplicationG1 { base: ark_base.encode(), scalar: ark_bigint.encode() }.encode();
        let builtin_actor_id: ActorId = H256::from(ACTOR_ID).cast();
        let gas_info = get_gas_info(builtin_actor_id, payload.clone());

        // Check the case of insufficient gas
        System::reset_events();
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(SIGNER),
            builtin_actor_id,
            payload.clone(),
            gas_info.min_limit / 2,
            0,
            false,
        ));

        run_to_next_block();

        // An error reply should have been sent.
        assert!(System::events().into_iter().any(|e| match e.event {
            RuntimeEvent::Gear(pallet_gear::Event::<Test>::UserMessageSent { message, .. }) => {
                message.destination() == SIGNER.cast()
                    && matches!(message.details(), Some(details) if details.to_reply_code()
                    == ReplyCode::Error(ErrorReplyReason::Execution(
                        SimpleExecutionError::RanOutOfGas,
                    )))
            }
            _ => false,
        }));

        // Check the computations are correct
        System::reset_events();

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(SIGNER),
            builtin_actor_id,
            payload,
            gas_info.min_limit,
            0,
            false,
        ));

        run_to_next_block();

        let response = match System::events().into_iter().find_map(|e| match e.event {
            RuntimeEvent::Gear(pallet_gear::Event::<Test>::UserMessageSent { message, .. }) => {
                assert_eq!(message.destination(), SIGNER.cast());
                assert!(matches!(message.details(), Some(details) if matches!(details.to_reply_code(), ReplyCode::Success(..))));

                Some(message.payload_bytes().to_vec())
            }

            _ => None,
        }) {
            Some(response) => response,
            _ => unreachable!(),
        };

        let builtin_result = match Response::decode(&mut response.as_slice()) {
            Ok(Response::ProjectiveMultiplicationG1(builtin_result)) => builtin_result,
            _ => unreachable!(),
        };

        let builtin_result = ArkScaleProjective::<G1>::decode(&mut builtin_result.as_slice()).unwrap();
        assert_eq!(result, builtin_result.0);
    });
}

#[test]
fn mul_projective_g2() {
    init_logger();

    new_test_ext().execute_with(|| {
        let mut rng = ark_std::test_rng();

        let bigint = BigInt::<3>::rand(&mut rng);
        let bigint = bigint.0.to_vec();
        let base = G2::rand(&mut rng);

        let result = <ark_bls12_381::g2::Config as SWCurveConfig>::mul_projective(&base, &bigint);

        let ark_bigint: ArkScale<Vec<u64>> = bigint.into();
        let ark_base: ArkScaleProjective<G2> = base.into();
        let payload = Request::ProjectiveMultiplicationG2 { base: ark_base.encode(), scalar: ark_bigint.encode() }.encode();
        let builtin_actor_id: ActorId = H256::from(ACTOR_ID).cast();
        let gas_info = get_gas_info(builtin_actor_id, payload.clone());

        // Check the case of insufficient gas
        System::reset_events();
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(SIGNER),
            builtin_actor_id,
            payload.clone(),
            gas_info.min_limit / 2,
            0,
            false,
        ));

        run_to_next_block();

        // An error reply should have been sent.
        assert!(System::events().into_iter().any(|e| match e.event {
            RuntimeEvent::Gear(pallet_gear::Event::<Test>::UserMessageSent { message, .. }) => {
                message.destination() == SIGNER.cast()
                    && matches!(message.details(), Some(details) if details.to_reply_code()
                    == ReplyCode::Error(ErrorReplyReason::Execution(
                        SimpleExecutionError::RanOutOfGas,
                    )))
            }
            _ => false,
        }));

        // Check the computations are correct
        System::reset_events();

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(SIGNER),
            builtin_actor_id,
            payload,
            gas_info.min_limit,
            0,
            false,
        ));

        run_to_next_block();

        let response = match System::events().into_iter().find_map(|e| match e.event {
            RuntimeEvent::Gear(pallet_gear::Event::<Test>::UserMessageSent { message, .. }) => {
                assert_eq!(message.destination(), SIGNER.cast());
                assert!(matches!(message.details(), Some(details) if matches!(details.to_reply_code(), ReplyCode::Success(..))));

                Some(message.payload_bytes().to_vec())
            }

            _ => None,
        }) {
            Some(response) => response,
            _ => unreachable!(),
        };

        let builtin_result = match Response::decode(&mut response.as_slice()) {
            Ok(Response::ProjectiveMultiplicationG2(builtin_result)) => builtin_result,
            _ => unreachable!(),
        };

        let builtin_result = ArkScaleProjective::<G2>::decode(&mut builtin_result.as_slice()).unwrap();
        assert_eq!(result, builtin_result.0);
    });
}

#[test]
fn aggregate_g1() {
    init_logger();

    new_test_ext().execute_with(|| {
        const COUNT: usize = 5;

        let mut rng = ark_std::test_rng();

        let points = (0..COUNT).map(|_| G1::rand(&mut rng)).collect::<Vec<_>>();
        let ark_points: ArkScale<Vec<G1>> = points.clone().into();
        let encoded_points = ark_points.encode();

        let payload = Request::AggregateG1 { points: encoded_points }.encode();
        let builtin_actor_id = ACTOR_ID.into();
        let gas_info = get_gas_info(builtin_actor_id, payload.clone());

        // Check the case of insufficient gas
        System::reset_events();
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(SIGNER),
            builtin_actor_id,
            payload.clone(),
            gas_info.min_limit / 2,
            0,
            false,
        ));

        run_to_next_block();

        // An error reply should have been sent.
        assert!(System::events().into_iter().any(|e| match e.event {
            RuntimeEvent::Gear(pallet_gear::Event::<Test>::UserMessageSent { message, .. }) => {
                message.destination() == SIGNER.cast()
                    && matches!(message.details(), Some(details) if details.to_reply_code()
                    == ReplyCode::Error(ErrorReplyReason::Execution(
                        SimpleExecutionError::RanOutOfGas,
                    )))
            }
            _ => false,
        }));

        // Check the computations are correct
        System::reset_events();

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(SIGNER),
            builtin_actor_id,
            payload,
            gas_info.min_limit,
            0,
            false,
        ));

        run_to_next_block();

        let response = match System::events().into_iter().find_map(|e| match e.event {
            RuntimeEvent::Gear(pallet_gear::Event::<Test>::UserMessageSent { message, .. }) => {
                assert_eq!(message.destination(), SIGNER.cast());
                assert!(matches!(message.details(), Some(details) if matches!(details.to_reply_code(), ReplyCode::Success(..))));

                Some(message.payload_bytes().to_vec())
            }

            _ => None,
        }) {
            Some(response) => response,
            _ => unreachable!(),
        };

        let builtin_result = match Response::decode(&mut response.as_slice()) {
            Ok(Response::AggregateG1(builtin_result)) => builtin_result,
            _ => unreachable!(),
        };

        let builtin_result = ArkScale::<G1>::decode(&mut builtin_result.as_slice()).unwrap();
        let point_first = points.first().unwrap();
        let point_aggregated = points
            .iter()
            .skip(1)
            .fold(*point_first, |aggregated, point| aggregated + *point);

        assert_eq!(point_aggregated, builtin_result.0);
    });
}

#[test]
fn map_to_g2affine() {
    init_logger();

    new_test_ext().execute_with(|| {
        let message = b"Hello, decentralized world!".to_vec();

        let payload = Request::MapToG2Affine { message: message.clone() }.encode();
        let builtin_actor_id: ActorId = H256::from(ACTOR_ID).cast();
        let gas_info = get_gas_info(builtin_actor_id, payload.clone());

        // Check the case of insufficient gas
        System::reset_events();
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(SIGNER),
            builtin_actor_id,
            payload.clone(),
            gas_info.min_limit / 2,
            0,
            false,
        ));

        run_to_next_block();

        // An error reply should have been sent.
        assert!(System::events().into_iter().any(|e| match e.event {
            RuntimeEvent::Gear(pallet_gear::Event::<Test>::UserMessageSent { message, .. }) => {
                message.destination() == SIGNER.cast()
                    && matches!(message.details(), Some(details) if details.to_reply_code()
                    == ReplyCode::Error(ErrorReplyReason::Execution(
                        SimpleExecutionError::RanOutOfGas,
                    )))
            }
            _ => false,
        }));

        // Check the computations are correct
        System::reset_events();

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(SIGNER),
            builtin_actor_id,
            payload,
            gas_info.min_limit,
            0,
            false,
        ));

        run_to_next_block();

        let response = match System::events().into_iter().find_map(|e| match e.event {
            RuntimeEvent::Gear(pallet_gear::Event::<Test>::UserMessageSent { message, .. }) => {
                assert_eq!(message.destination(), SIGNER.cast());
                assert!(matches!(message.details(), Some(details) if matches!(details.to_reply_code(), ReplyCode::Success(..))));

                Some(message.payload_bytes().to_vec())
            }

            _ => None,
        }) {
            Some(response) => response,
            _ => unreachable!(),
        };

        let builtin_result = match Response::decode(&mut response.as_slice()) {
            Ok(Response::MapToG2Affine(builtin_result)) => builtin_result,
            _ => unreachable!(),
        };

        let builtin_result = ArkScale::<G2Affine>::decode(&mut builtin_result.as_slice()).unwrap();

        assert!(
            matches!(
                MapToCurveBasedHasher::<G2, DefaultFieldHasher<sha2::Sha256>, WBMap>::new(DST_G2),
                Ok(mapper) if matches!(
                    mapper.hash(&message),
                    Ok(point) if point == builtin_result.0
                )
            )
        );
    });
}
