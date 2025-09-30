// This file is part of Gear.
//
// Copyright (C) 2025 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! BLS12-381 builtin actor implementation.
//!
//! The main function of the module is `process_bls12_381_dispatch` which
//! processes incoming dispatches to the bls12-381 builtin actor.

pub use gbuiltin_bls381::{Request as Bls12_381Request, Response as Bls12_381Response};

use ark_bls12_381::{G1Projective as G1, G2Affine, G2Projective as G2};
use ark_ec::{
    bls12::Bls12Config,
    hashing::{HashToCurve, curve_maps::wb, map_to_curve_hasher::MapToCurveBasedHasher},
};
use ark_ff::fields::field_hashers::DefaultFieldHasher;
use ark_scale::{ArkScale, HOST_CALL};
use ark_serialize::{CanonicalDeserialize, Compress, Validate};
use builtins_common::{
    BuiltinActorError, BuiltinContext,
    bls12_381::{self, Bls12_381Ops, BlsOpsGasCost},
};
use gbuiltin_bls381::{REQUEST_FINAL_EXPONENTIATION, REQUEST_MULTI_MILLER_LOOP};
use gear_core::{ids::ActorId, message::StoredDispatch, str::LimitedStr};
use parity_scale_codec::{Decode, Encode};

/// The id of the BLS12-381 builtin actor.
pub const BLS12_381_ID: ActorId = ActorId::new(*b"modl/bia/bls12-381/v-\x01\0/\0\0\0\0\0\0\0\0");

// const IS_COMPRESSED: Compress = ark_scale::is_compressed(HOST_CALL);
// const IS_VALIDATED: Validate = ark_scale::is_validated(HOST_CALL);

type ArkScaleLocal<T> = ark_scale::ArkScale<T, { ark_scale::HOST_CALL }>;

struct BlsOpsGasCostsImpl;

impl BlsOpsGasCost for BlsOpsGasCostsImpl {
    fn decode_bytes(len: u32) -> u64 {
        0
    }

    fn bls12_381_multi_miller_loop(count: u32) -> u64 {
        0
    }

    fn bls12_381_final_exponentiation() -> u64 {
        0
    }

    fn bls12_381_msm_g1(count: u32) -> u64 {
        0
    }

    fn bls12_381_msm_g2(count: u32) -> u64 {
        0
    }

    fn bls12_381_mul_projective_g1(count: u32) -> u64 {
        0
    }

    fn bls12_381_mul_projective_g2(count: u32) -> u64 {
        0
    }

    fn bls12_381_aggregate_g1(count: u32) -> u64 {
        0
    }

    fn bls12_381_map_to_g2affine(len: u32) -> u64 {
        0
    }
}

/// Processes a dispatch message sent to the BLS12-381 builtin actor.
pub(crate) fn process_bls12_381_dispatch(
    dispatch: &StoredDispatch,
    context: &mut BuiltinContext,
) -> Result<Bls12_381Response, BuiltinActorError> {
    let mut payload = dispatch.payload_bytes();

    match payload.first().copied() {
        Some(REQUEST_MULTI_MILLER_LOOP) => {
            bls12_381::multi_miller_loop::<BlsOpsGasCostsImpl>(&payload[1..], context, |a, b| {
                Bls12_381Ops::multi_miller_loop(a, b)
            })
            .map(Bls12_381Response::MultiMillerLoop)
        }
        Some(REQUEST_FINAL_EXPONENTIATION) => {
            bls12_381::final_exponentiation::<BlsOpsGasCostsImpl>(&payload[1..], context, |f| {
                Bls12_381Ops::final_exponentiation(f)
            })
            .map(Bls12_381Response::FinalExponentiation)
        }
        _ => unreachable!("todo [sab]"), /* Bls12_381Request::FinalExponentiation { f } =>
                                          * final_exponentiation(f),
                                          * Bls12_381Request::MultiScalarMultiplicationG1 {
                                          * bases, scalars } => msm_g1(bases, scalars),
                                          * Bls12_381Request::MultiScalarMultiplicationG2 {
                                          * bases, scalars } => msm_g2(bases, scalars),
                                          * Bls12_381Request::ProjectiveMultiplicationG1 { base,
                                          * scalar } => {
                                          *     projective_multiplication_g1(base, scalar)
                                          * }
                                          * Bls12_381Request::ProjectiveMultiplicationG2 { base,
                                          * scalar } => {
                                          *     projective_multiplication_g2(base, scalar)
                                          * }
                                          * Bls12_381Request::AggregateG1 { points } =>
                                          * aggregate_g1(points),
                                          * Bls12_381Request::MapToG2Affine { message } =>
                                          * map_to_g2affine(message), */
    }
}

// fn final_exponentiation(f: Vec<u8>) -> Result<Bls12_381Response,
// BuiltinActorError> {
//     match bls12_381::host_calls::bls12_381_final_exponentiation(f) {
//         Ok(result) => Ok(Bls12_381Response::FinalExponentiation(result)),
//         Err(_) => Err(BuiltinActorError::Custom(LimitedStr::from_small_str(
//             "Final exponentiation: computation error",
//         ))),
//     }
// }

// fn msm_g1(bases: Vec<u8>, scalars: Vec<u8>) -> Result<Bls12_381Response,
// BuiltinActorError> {     msm(
//         bases,
//         scalars,
//         |_count| 0,
//         |bases, scalars| {
//             bls12_381::host_calls::bls12_381_msm_g1(bases, scalars)
//                 .map(Bls12_381Response::MultiScalarMultiplicationG1)
//         },
//     )
// }

// fn msm_g2(bases: Vec<u8>, scalars: Vec<u8>) -> Result<Bls12_381Response,
// BuiltinActorError> {     msm(
//         bases,
//         scalars,
//         |_count| 0,
//         |bases, scalars| {
//             bls12_381::host_calls::bls12_381_msm_g2(bases, scalars)
//                 .map(Bls12_381Response::MultiScalarMultiplicationG2)
//         },
//     )
// }

// fn msm(
//     bases: Vec<u8>,
//     scalars: Vec<u8>,
//     _gas_to_spend: impl FnOnce(u32) -> u64,
//     call: impl FnOnce(Vec<u8>, Vec<u8>) -> Result<Bls12_381Response, ()>,
// ) -> Result<Bls12_381Response, BuiltinActorError> {
//     let mut slice = bases.as_slice();
//     let mut reader = ark_scale::rw::InputAsRead(&mut slice);
//     let Ok(count) = u64::deserialize_with_mode(&mut reader, IS_COMPRESSED,
// IS_VALIDATED) else {         log::debug!("Failed to decode items count in
// bases");

//         return Err(BuiltinActorError::DecodingError);
//     };

//     let mut slice = scalars.as_slice();
//     let mut reader = ark_scale::rw::InputAsRead(&mut slice);
//     match u64::deserialize_with_mode(&mut reader, IS_COMPRESSED,
// IS_VALIDATED) {         Ok(count_b) if count_b != count => {
//             return Err(BuiltinActorError::Custom(LimitedStr::from_small_str(
//                 "Multi scalar multiplication: uneven item count",
//             )));
//         }
//         Err(_) => {
//             log::debug!("Failed to decode items count in scalars");

//             return Err(BuiltinActorError::DecodingError);
//         }
//         Ok(_) => (),
//     }

//     match call(bases, scalars) {
//         Ok(result) => Ok(result),
//         Err(_) => Err(BuiltinActorError::Custom(LimitedStr::from_small_str(
//             "Multi scalar multiplication: computation error",
//         ))),
//     }
// }

// fn projective_multiplication_g1(
//     base: Vec<u8>,
//     scalar: Vec<u8>,
// ) -> Result<Bls12_381Response, BuiltinActorError> {
//     projective_multiplication(
//         base,
//         scalar,
//         |_count| 0,
//         |base, scalar| {
//             bls12_381::host_calls::bls12_381_mul_projective_g1(base, scalar)
//                 .map(Bls12_381Response::ProjectiveMultiplicationG1)
//         },
//     )
// }

// fn projective_multiplication_g2(
//     base: Vec<u8>,
//     scalar: Vec<u8>,
// ) -> Result<Bls12_381Response, BuiltinActorError> {
//     projective_multiplication(
//         base,
//         scalar,
//         |_count| 0,
//         |base, scalar| {
//             bls12_381::host_calls::bls12_381_mul_projective_g2(base, scalar)
//                 .map(Bls12_381Response::ProjectiveMultiplicationG2)
//         },
//     )
// }

// fn projective_multiplication(
//     base: Vec<u8>,
//     scalar: Vec<u8>,
//     _gas_to_spend: impl FnOnce(u32) -> u64,
//     call: impl FnOnce(Vec<u8>, Vec<u8>) -> Result<Bls12_381Response, ()>,
// ) -> Result<Bls12_381Response, BuiltinActorError> {
//     let mut slice = scalar.as_slice();
//     let mut reader = ark_scale::rw::InputAsRead(&mut slice);
//     let Ok(_count) = u64::deserialize_with_mode(&mut reader, IS_COMPRESSED,
// IS_VALIDATED) else {         log::debug!("Failed to decode items count in
// scalar");

//         return Err(BuiltinActorError::DecodingError);
//     };

//     call(base, scalar).map_err(|_| {
//         BuiltinActorError::Custom(LimitedStr::from_small_str(
//             "Projective multiplication: computation error",
//         ))
//     })
// }

// fn aggregate_g1(points: Vec<u8>) -> Result<Bls12_381Response,
// BuiltinActorError> {     let mut slice = points.as_slice();
//     let mut reader = ark_scale::rw::InputAsRead(&mut slice);
//     let Ok(_count) = u64::deserialize_with_mode(&mut reader, IS_COMPRESSED,
// IS_VALIDATED) else {         log::debug!("Failed to decode items count in
// points");

//         return Err(BuiltinActorError::DecodingError);
//     };

//     let ArkScale(points) = ArkScaleLocal::<Vec<G1>>::decode(&mut &points[..])
//         .map_err(|_| BuiltinActorError::DecodingError)?;
//     let point_first =
//         points
//             .first()
//             .ok_or(BuiltinActorError::Custom(LimitedStr::from_small_str(
//                 "The array of G1-points is empty",
//             )))?;
//     let point_aggregated = points
//         .iter()
//         .skip(1)
//         .fold(*point_first, |aggregated, point| aggregated + *point);
//     let res = ArkScaleLocal::<G1>::from(point_aggregated).encode();

//     Ok(Bls12_381Response::AggregateG1(res))
// }

// fn map_to_g2affine(message: Vec<u8>) -> Result<Bls12_381Response,
// BuiltinActorError> {     type WBMap = wb::WBMap<<ark_bls12_381::Config as
// Bls12Config>::G2Config>;

//     const DST_G2: &[u8] = b"BLS_SIG_BLS12381G2_XMD:SHA-256_SSWU_RO_POP_";

//     let mapper = MapToCurveBasedHasher::<G2,
// DefaultFieldHasher<sha2::Sha256>, WBMap>::new(DST_G2)         .map_err(|_| {
//             BuiltinActorError::Custom(LimitedStr::from_small_str(
//                 "Failed to create `MapToCurveBasedHasher`",
//             ))
//         })?;
//     let point = mapper.hash(&message).map_err(|_| {
//         BuiltinActorError::Custom(LimitedStr::from_small_str(
//             "Failed to map a message to a G2-point",
//         ))
//     })?;
//     let res = ArkScaleLocal::<G2Affine>::from(point).encode();

//     Ok(Bls12_381Response::MapToG2Affine(res))
// }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DEFAULT_USER_ALICE, Log, Program, System};
    use ark_bls12_381::{Bls12_381, G1Affine, G2Affine};
    use ark_ec::{
        Group, ScalarMul, VariableBaseMSM,
        pairing::Pairing,
        short_weierstrass::{Projective as SWProjective, SWCurveConfig},
    };
    use ark_ff::{UniformRand, biginteger::BigInt};
    use ark_scale::hazmat::ArkScaleProjective;
    use ark_std::test_rng;
    use demo_constructor::{Arg, Call, Calls, Scheme, WASM_BINARY};
    use gear_common::Origin;
    use std::ops::Mul;

    type ScalarFieldG1 = <G1 as Group>::ScalarField;
    type ScalarFieldG2 = <G2 as Group>::ScalarField;

    fn create_proxy_program(
        sys: &System,
        proxy_id: ActorId,
        builtin_req: Vec<u8>,
        reply_receiver: ActorId,
    ) -> Program<'_> {
        let proxy_scheme = Scheme::predefined(
            // init: do nothing
            Calls::builder().noop(),
            // handle: send message to bls12-381 builtin
            Calls::builder().add_call(Call::Send(
                Arg::new(BLS12_381_ID.into_bytes()),
                Arg::new(builtin_req),
                None,
                Arg::new(0u128),
                Arg::new(0u32),
            )),
            // handle_reply: load reply payload and forward it to original sender
            Calls::builder()
                .add_call(Call::LoadBytes)
                .add_call(Call::StoreVec("reply_payload".to_string()))
                .add_call(Call::Send(
                    Arg::new(reply_receiver.into_bytes()),
                    Arg::get("reply_payload"),
                    Some(Arg::new(0)),
                    Arg::new(0u128),
                    Arg::new(0u32),
                )),
            // handle_signal: noop
            Calls::builder(),
        );

        let proxy_program = Program::from_binary_with_id(sys, proxy_id, WASM_BINARY);

        // Initialize proxy with the scheme
        let init_mid = proxy_program.send(reply_receiver, proxy_scheme);
        let res = sys.run_next_block();
        assert!(res.succeed.contains(&init_mid));

        proxy_program
    }

    #[test]
    fn test_multi_miller_loop() {
        let sys = System::new();

        let alice_id = ActorId::from(DEFAULT_USER_ALICE);
        let proxy_pid = ActorId::new([3; 32]);

        // -----------------------------------------------------------------------
        // ------------------------ Prepare payload ------------------------------
        // -----------------------------------------------------------------------
        let mut rng = test_rng();
        let message: G1Affine = G1::rand(&mut rng).into();
        let a: ArkScaleLocal<Vec<<Bls12_381 as Pairing>::G1Affine>> = vec![message].into();

        let priv_key: ScalarFieldG2 = UniformRand::rand(&mut rng);
        let generator = G2::generator();
        let pub_key: G2Affine = generator.mul(priv_key).into();
        let b: ArkScaleLocal<Vec<<Bls12_381 as Pairing>::G2Affine>> = vec![pub_key].into();

        let multi_miller_req = Bls12_381Request::MultiMillerLoop {
            a: a.encode(),
            b: b.encode(),
        };

        // -----------------------------------------------------------------------
        // ------------------------ Create proxy program -------------------------
        // -----------------------------------------------------------------------
        let proxy_program =
            create_proxy_program(&sys, proxy_pid, multi_miller_req.encode(), alice_id);

        // -----------------------------------------------------------------------
        // ------------------------- Trigger builtin -----------------------------
        // -----------------------------------------------------------------------
        let mid = proxy_program.send_bytes(alice_id, b"");
        let res = sys.run_next_block();
        assert!(res.succeed.contains(&mid));

        // -----------------------------------------------------------------------
        // ------------------------- Check response ------------------------------
        // -----------------------------------------------------------------------
        assert!(res.contains(&Log::builder().source(proxy_pid).dest(alice_id)));

        let mut logs = res.decoded_log();
        let response = logs.pop().expect("no log found");

        assert!(matches!(
            response.payload(),
            Bls12_381Response::MultiMillerLoop(_)
        ));
    }

    #[test]
    fn test_final_exponentiation() {
        let sys = System::new();

        let alice_id = ActorId::from(DEFAULT_USER_ALICE);
        let proxy_pid = ActorId::new([3; 32]);

        // -----------------------------------------------------------------------
        // ------------------------ Prepare payload ------------------------------
        // -----------------------------------------------------------------------
        let mut rng = ark_std::test_rng();

        let message: G1Affine = G1::rand(&mut rng).into();
        let priv_key: ScalarFieldG2 = UniformRand::rand(&mut rng);
        let generator: G2 = G2::generator();
        let pub_key: G2Affine = generator.mul(priv_key).into();

        let loop_result = <Bls12_381 as Pairing>::multi_miller_loop(vec![message], vec![pub_key]);
        let expected = <Bls12_381 as Pairing>::final_exponentiation(loop_result);

        let f: ArkScale<<Bls12_381 as Pairing>::TargetField> = loop_result.0.into();
        let final_expon_req = Bls12_381Request::FinalExponentiation { f: f.encode() };

        // -----------------------------------------------------------------------
        // ------------------------ Create proxy program -------------------------
        // -----------------------------------------------------------------------
        let proxy_program =
            create_proxy_program(&sys, proxy_pid, final_expon_req.encode(), alice_id);

        // -----------------------------------------------------------------------
        // ------------------------- Trigger builtin -----------------------------
        // -----------------------------------------------------------------------
        let mid = proxy_program.send_bytes(alice_id, b"");
        let res = sys.run_next_block();
        assert!(res.succeed.contains(&mid));

        // -----------------------------------------------------------------------
        // ------------------------- Check response ------------------------------
        // -----------------------------------------------------------------------
        assert!(res.contains(&Log::builder().source(proxy_pid).dest(alice_id)));

        let mut logs = res.decoded_log();
        let response = logs.pop().expect("no log found");

        if let Bls12_381Response::FinalExponentiation(result_bytes) = response.payload() {
            let actual = ArkScaleLocal::<<Bls12_381 as Pairing>::TargetField>::decode(
                &mut result_bytes.as_ref(),
            )
            .expect("failed to decode result");

            assert!(matches!(expected, Some(inner) if inner.0 == actual.0));
        } else {
            panic!("unexpected response");
        }
    }

    // #[test]
    // fn test_msm_g1() {
    //     let sys = System::new();

    //     let alice_actor_id = ActorId::from(DEFAULT_USER_ALICE);
    //     let proxy_pid = ActorId::new([3; 32]);

    //     // -----------------------------------------------------------------------
    //     // ------------------------ Prepare payload
    // ------------------------------     // -----------------------------------------------------------------------
    //     let mut rng = test_rng();

    //     let count = 5usize;
    //     let scalars = (0..count)
    //         .map(|_| ScalarFieldG1::rand(&mut rng))
    //         .collect::<Vec<_>>();

    //     let bases = G1::batch_convert_to_mul_base(
    //         &(0..count).map(|_| G1::rand(&mut rng)).collect::<Vec<_>>(),
    //     );

    //     let faulty_ark_scalars: ArkScaleLocal<Vec<ScalarFieldG1>> =
    // scalars[1..].to_vec().into();     let ark_bases:
    // ArkScaleLocal<Vec<G1Affine>> = bases.clone().into();

    //     let faulty_msm_g1_req = Bls12_381Request::MultiScalarMultiplicationG1
    // {         bases: ark_bases.encode(),
    //         scalars: faulty_ark_scalars.encode(),
    //     };

    //     // -----------------------------------------------------------------------
    //     // ----------------------- Create a faulty proxy
    // -------------------------     // -----------------------------------------------------------------------
    //     // Because of the impl of the demo_constructor, we have to waste 1
    // program as we     // cannot predefine `handle_reply` without defining
    // `handle` (using     // `Scheme::predefined`). So we have to define a
    // proxy with `handle` sending the     // faulty request
    //     let proxy_program = create_proxy_program(
    //         &sys,
    //         gprimitives::H256::random().cast(),
    //         faulty_msm_g1_req.encode(),
    //         alice_actor_id,
    //     );

    //     // -----------------------------------------------------------------------
    //     // ------------------------- Trigger builtin
    // -----------------------------     // -----------------------------------------------------------------------
    //     let mid = proxy_program.send_bytes(alice_actor_id, b"");
    //     let res = sys.run_next_block();
    //     assert!(res.succeed.contains(&mid));

    //     // -----------------------------------------------------------------------
    //     // ----------------------- Check error response
    // --------------------------     // -----------------------------------------------------------------------
    //     let err_payload =
    //         LimitedStr::from_small_str("Multi scalar multiplication: uneven
    // item count")             .into_inner()
    //             .into_owned()
    //             .into_bytes();
    //     assert!(
    //         res.contains(
    //             &Log::builder()
    //                 .source(proxy_program.id())
    //                 .dest(alice_actor_id)
    //                 .payload_bytes(err_payload)
    //         )
    //     );

    //     // -----------------------------------------------------------------------
    //     // ------------------------ Prepare valid payload
    // -----------------------     // -----------------------------------------------------------------------
    //     let expected =
    //         <SWProjective<ark_bls12_381::g1::Config> as
    // VariableBaseMSM>::msm(&bases, &scalars)             .expect("msm
    // expected result generation failed");

    //     let ark_scalars: ArkScaleLocal<Vec<ScalarFieldG1>> = scalars.into();
    //     let ark_bases: ArkScaleLocal<Vec<G1Affine>> = bases.into();

    //     let msm_g1_req = Bls12_381Request::MultiScalarMultiplicationG1 {
    //         bases: ark_bases.encode(),
    //         scalars: ark_scalars.encode(),
    //     };

    //     // -----------------------------------------------------------------------
    //     // ------------------------ Create proxy program
    // -------------------------     // -----------------------------------------------------------------------
    //     let proxy_program =
    //         create_proxy_program(&sys, proxy_pid, msm_g1_req.encode(),
    // alice_actor_id);

    //     // -----------------------------------------------------------------------
    //     // ------------------------- Trigger builtin
    // -----------------------------     // -----------------------------------------------------------------------
    //     let mid = proxy_program.send_bytes(alice_actor_id, b"");
    //     let res = sys.run_next_block();
    //     assert!(res.succeed.contains(&mid));

    //     // -----------------------------------------------------------------------
    //     // ------------------------- Check response
    // ------------------------------     // -----------------------------------------------------------------------
    //     let mut logs = res.decoded_log();
    //     let response = logs.pop().expect("no log found");

    //     if let Bls12_381Response::MultiScalarMultiplicationG1(result_bytes) =
    // response.payload() {         let actual =
    // ArkScaleProjective::<G1>::decode(&mut result_bytes.as_ref())
    //             .expect("failed to decode result");

    //         assert_eq!(actual.0, expected);
    //     } else {
    //         panic!("unexpected response");
    //     }
    // }

    // #[test]
    // fn test_msm_g2() {
    //     let sys = System::new();

    //     let alice_actor_id = ActorId::from(DEFAULT_USER_ALICE);
    //     let proxy_pid = ActorId::new([3; 32]);

    //     // -----------------------------------------------------------------------
    //     // ------------------------ Prepare payload
    // ------------------------------     // -----------------------------------------------------------------------
    //     let mut rng = test_rng();

    //     let count = 5usize;
    //     let scalars = (0..count)
    //         .map(|_| ScalarFieldG2::rand(&mut rng))
    //         .collect::<Vec<_>>();

    //     let bases = G2::batch_convert_to_mul_base(
    //         &(0..count).map(|_| G2::rand(&mut rng)).collect::<Vec<_>>(),
    //     );

    //     let faulty_ark_scalars: ArkScaleLocal<Vec<ScalarFieldG2>> =
    // scalars[1..].to_vec().into();     let ark_bases:
    // ArkScaleLocal<Vec<G2Affine>> = bases.clone().into();

    //     let faulty_msm_g2_req = Bls12_381Request::MultiScalarMultiplicationG2
    // {         bases: ark_bases.encode(),
    //         scalars: faulty_ark_scalars.encode(),
    //     };

    //     // -----------------------------------------------------------------------
    //     // ----------------------- Create a faulty proxy
    // -------------------------     // -----------------------------------------------------------------------
    //     // Because of the impl of the demo_constructor, we have to waste 1
    // program as we     // cannot predefine `handle_reply` without defining
    // `handle` (using     // `Scheme::predefined`). So we have to define a
    // proxy with `handle` sending the     // faulty request
    //     let proxy_program = create_proxy_program(
    //         &sys,
    //         gprimitives::H256::random().cast(),
    //         faulty_msm_g2_req.encode(),
    //         alice_actor_id,
    //     );

    //     // -----------------------------------------------------------------------
    //     // ------------------------- Trigger builtin
    // -----------------------------     // -----------------------------------------------------------------------
    //     let mid = proxy_program.send_bytes(alice_actor_id, b"");
    //     let res = sys.run_next_block();
    //     assert!(res.succeed.contains(&mid));

    //     // -----------------------------------------------------------------------
    //     // ----------------------- Check error response
    // --------------------------     // -----------------------------------------------------------------------
    //     let err_payload =
    //         LimitedStr::from_small_str("Multi scalar multiplication: uneven
    // item count")             .into_inner()
    //             .into_owned()
    //             .into_bytes();
    //     assert!(
    //         res.contains(
    //             &Log::builder()
    //                 .source(proxy_program.id())
    //                 .dest(alice_actor_id)
    //                 .payload_bytes(err_payload)
    //         )
    //     );

    //     // -----------------------------------------------------------------------
    //     // ------------------------ Prepare valid payload
    // -----------------------     // -----------------------------------------------------------------------
    //     let expected =
    //         <SWProjective<ark_bls12_381::g2::Config> as
    // VariableBaseMSM>::msm(&bases, &scalars)             .expect("msm
    // expected result generation failed");

    //     let ark_scalars: ArkScaleLocal<Vec<ScalarFieldG2>> = scalars.into();
    //     let ark_bases: ArkScaleLocal<Vec<G2Affine>> = bases.into();

    //     let msm_g2_req = Bls12_381Request::MultiScalarMultiplicationG2 {
    //         bases: ark_bases.encode(),
    //         scalars: ark_scalars.encode(),
    //     };

    //     // -----------------------------------------------------------------------
    //     // ------------------------ Create proxy program
    // -------------------------     // -----------------------------------------------------------------------
    //     let proxy_program =
    //         create_proxy_program(&sys, proxy_pid, msm_g2_req.encode(),
    // alice_actor_id);

    //     // -----------------------------------------------------------------------
    //     // ------------------------- Trigger builtin
    // -----------------------------     // -----------------------------------------------------------------------
    //     let mid = proxy_program.send_bytes(alice_actor_id, b"");
    //     let res = sys.run_next_block();
    //     assert!(res.succeed.contains(&mid));

    //     // -----------------------------------------------------------------------
    //     // ------------------------- Check response
    // ------------------------------     // -----------------------------------------------------------------------
    //     let mut logs = res.decoded_log();
    //     let response = logs.pop().expect("no log found");

    //     if let Bls12_381Response::MultiScalarMultiplicationG2(result_bytes) =
    // response.payload() {         let actual =
    // ArkScaleProjective::<G2>::decode(&mut result_bytes.as_ref())
    //             .expect("failed to decode result");

    //         assert_eq!(actual.0, expected);
    //     } else {
    //         panic!("unexpected response");
    //     }
    // }

    // #[test]
    // fn test_projective_multiplication_g1() {
    //     let sys = System::new();

    //     let alice_actor_id = ActorId::from(DEFAULT_USER_ALICE);
    //     let proxy_pid = ActorId::new([3; 32]);

    //     // -----------------------------------------------------------------------
    //     // ------------------------ Prepare payload
    // ------------------------------     // -----------------------------------------------------------------------
    //     let mut rng = test_rng();

    //     let bigint = BigInt::<3>::rand(&mut rng).0.to_vec();
    //     let base = G1::rand(&mut rng);

    //     let expected = <ark_bls12_381::g1::Config as
    // SWCurveConfig>::mul_projective(&base, &bigint);

    //     let ark_bigint: ArkScaleLocal<Vec<u64>> = bigint.into();
    //     let ark_base: ArkScaleProjective<G1> = base.into();

    //     let proj_mul_g1_req = Bls12_381Request::ProjectiveMultiplicationG1 {
    //         base: ark_base.encode(),
    //         scalar: ark_bigint.encode(),
    //     };

    //     // -----------------------------------------------------------------------
    //     // ------------------------ Create proxy program
    // -------------------------     // -----------------------------------------------------------------------
    //     let proxy_program =
    //         create_proxy_program(&sys, proxy_pid, proj_mul_g1_req.encode(),
    // alice_actor_id);

    //     // -----------------------------------------------------------------------
    //     // ------------------------- Trigger builtin
    // -----------------------------     // -----------------------------------------------------------------------
    //     let mid = proxy_program.send_bytes(alice_actor_id, b"");
    //     let res = sys.run_next_block();
    //     assert!(res.succeed.contains(&mid));

    //     // -----------------------------------------------------------------------
    //     // ------------------------- Check response
    // ------------------------------     // -----------------------------------------------------------------------
    //     let mut logs = res.decoded_log();
    //     let response = logs.pop().expect("no log found");

    //     if let Bls12_381Response::ProjectiveMultiplicationG1(result_bytes) =
    // response.payload() {         let actual =
    // ArkScaleProjective::<G1>::decode(&mut result_bytes.as_ref())
    //             .expect("failed to decode result");

    //         assert_eq!(actual.0, expected);
    //     } else {
    //         panic!("unexpected response");
    //     }
    // }

    // #[test]
    // fn test_projective_multiplication_g2() {
    //     let sys = System::new();

    //     let alice_actor_id = ActorId::from(DEFAULT_USER_ALICE);
    //     let proxy_pid = ActorId::new([3; 32]);

    //     // -----------------------------------------------------------------------
    //     // ------------------------ Prepare payload
    // ------------------------------     // -----------------------------------------------------------------------
    //     let mut rng = test_rng();

    //     let bigint = BigInt::<3>::rand(&mut rng).0.to_vec();
    //     let base = G2::rand(&mut rng);

    //     let expected = <ark_bls12_381::g2::Config as
    // SWCurveConfig>::mul_projective(&base, &bigint);

    //     let ark_bigint: ArkScaleLocal<Vec<u64>> = bigint.into();
    //     let ark_base: ArkScaleProjective<G2> = base.into();

    //     let proj_mul_g2_req = Bls12_381Request::ProjectiveMultiplicationG2 {
    //         base: ark_base.encode(),
    //         scalar: ark_bigint.encode(),
    //     };

    //     // -----------------------------------------------------------------------
    //     // ------------------------ Create proxy program
    // -------------------------     // -----------------------------------------------------------------------
    //     let proxy_program =
    //         create_proxy_program(&sys, proxy_pid, proj_mul_g2_req.encode(),
    // alice_actor_id);

    //     // -----------------------------------------------------------------------
    //     // ------------------------- Trigger builtin
    // -----------------------------     // -----------------------------------------------------------------------
    //     let mid = proxy_program.send_bytes(alice_actor_id, b"");
    //     let res = sys.run_next_block();
    //     assert!(res.succeed.contains(&mid));

    //     // -----------------------------------------------------------------------
    //     // ------------------------- Check response
    // ------------------------------     // -----------------------------------------------------------------------
    //     let mut logs = res.decoded_log();
    //     let response = logs.pop().expect("no log found");

    //     if let Bls12_381Response::ProjectiveMultiplicationG2(result_bytes) =
    // response.payload() {         let actual =
    // ArkScaleProjective::<G2>::decode(&mut result_bytes.as_ref())
    //             .expect("failed to decode result");

    //         assert_eq!(actual.0, expected);
    //     } else {
    //         panic!("unexpected response");
    //     }
    // }

    // #[test]
    // fn test_aggregate_g1() {
    //     let sys = System::new();

    //     let alice_actor_id = ActorId::from(DEFAULT_USER_ALICE);
    //     let proxy_pid = ActorId::new([3; 32]);

    //     // -----------------------------------------------------------------------
    //     // ------------------------ Prepare payload
    // ------------------------------     // -----------------------------------------------------------------------
    //     let mut rng = test_rng();

    //     const COUNT: usize = 5;

    //     let points = (0..COUNT).map(|_| G1::rand(&mut
    // rng)).collect::<Vec<_>>();     let ark_points: ArkScaleLocal<Vec<G1>>
    // = points.clone().into();

    //     let aggregate_g1_req = Bls12_381Request::AggregateG1 {
    //         points: ark_points.encode(),
    //     };

    //     // -----------------------------------------------------------------------
    //     // ------------------------ Create proxy program
    // -------------------------     // -----------------------------------------------------------------------
    //     let proxy_program =
    //         create_proxy_program(&sys, proxy_pid, aggregate_g1_req.encode(),
    // alice_actor_id);

    //     // -----------------------------------------------------------------------
    //     // ------------------------- Trigger builtin
    // -----------------------------     // -----------------------------------------------------------------------
    //     let mid = proxy_program.send_bytes(alice_actor_id, b"");
    //     let res = sys.run_next_block();
    //     assert!(res.succeed.contains(&mid));

    //     // -----------------------------------------------------------------------
    //     // ------------------------- Check response
    // ------------------------------     // -----------------------------------------------------------------------
    //     let mut logs = res.decoded_log();
    //     let response = logs.pop().expect("no log found");

    //     if let Bls12_381Response::AggregateG1(result_bytes) =
    // response.payload() {         let actual =
    // ArkScaleLocal::<G1>::decode(&mut result_bytes.as_ref())             
    // .expect("failed to decode result");

    //         let point_first = points.first().unwrap();
    //         let expected = points
    //             .iter()
    //             .skip(1)
    //             .fold(*point_first, |aggregated, point| aggregated + *point);

    //         assert_eq!(actual.0, expected);
    //     } else {
    //         panic!("unexpected response");
    //     }
    // }

    // #[test]
    // fn test_map_to_g2affine() {
    //     let sys = System::new();

    //     let alice_actor_id = ActorId::from(DEFAULT_USER_ALICE);
    //     let proxy_pid = ActorId::new([3; 32]);

    //     // -----------------------------------------------------------------------
    //     // ------------------------ Prepare payload
    // ------------------------------     // -----------------------------------------------------------------------
    //     let message = b"Hello, decentralized world!".to_vec();

    //     let map_to_g2_req = Bls12_381Request::MapToG2Affine {
    //         message: message.clone(),
    //     };

    //     // -----------------------------------------------------------------------
    //     // ------------------------ Create proxy program
    // -------------------------     // -----------------------------------------------------------------------
    //     let proxy_program =
    //         create_proxy_program(&sys, proxy_pid, map_to_g2_req.encode(),
    // alice_actor_id);

    //     // -----------------------------------------------------------------------
    //     // ------------------------- Trigger builtin
    // -----------------------------     // -----------------------------------------------------------------------
    //     let mid = proxy_program.send_bytes(alice_actor_id, b"");
    //     let res = sys.run_next_block();
    //     assert!(res.succeed.contains(&mid));

    //     // -----------------------------------------------------------------------
    //     // ------------------------- Check response
    // ------------------------------     // -----------------------------------------------------------------------
    //     let mut logs = res.decoded_log();
    //     let response = logs.pop().expect("no log found");

    //     if let Bls12_381Response::MapToG2Affine(result_bytes) =
    // response.payload() {         let actual =
    // ArkScaleLocal::<G2Affine>::decode(&mut result_bytes.as_ref())
    //             .expect("failed to decode result");

    //         // Verify the result matches what arkworks would produce
    //         type WBMap = wb::WBMap<<ark_bls12_381::Config as
    // Bls12Config>::G2Config>;         const DST_G2: &[u8] =
    // b"BLS_SIG_BLS12381G2_XMD:SHA-256_SSWU_RO_POP_";

    //         let mapper =
    //             MapToCurveBasedHasher::<G2, DefaultFieldHasher<sha2::Sha256>,
    // WBMap>::new(DST_G2)                 .expect("mapper creation
    // failed");         let expected = mapper.hash(&message).expect("hash
    // to curve failed");

    //         assert_eq!(actual.0, expected);
    //     } else {
    //         panic!("unexpected response");
    //     }
    // }
}
