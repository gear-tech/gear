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

use super::*;
use ark_bls12_381::{Bls12_381, G1Affine, G2Affine};
use ark_ec::pairing::Pairing;
use gbuiltin_bls381::*;
use gstd::{
    ActorId,
    codec::{Decode, Encode},
    msg,
    prelude::*,
};

type ArkScale<T> = ark_scale::ArkScale<T, { ark_scale::HOST_CALL }>;

const BUILTIN_BLS381: ActorId = ActorId::new(*b"modl/bia/bls12-381\0\0\0\0\0\0\0/v-\x01\0\0\0");

#[allow(dead_code)]
#[derive(Default)]
pub struct Contract {
    g2_gen: G2Affine,
    pub_keys: Vec<G2Affine>,
    aggregate_pub_key: G2Affine,
    miller_out: (
        // encoded ArkScale::<MillerLoopOutput<Bls12_381>>
        Option<Vec<u8>>,
        Option<Vec<u8>>,
    ),
}

static mut CONTRACT: Option<Contract> = None;

#[unsafe(no_mangle)]
extern "C" fn init() {
    let init_msg: InitMessage = msg::load().expect("Unable to decode `InitMessage`");

    let g2_gen = <ArkScale<<Bls12_381 as Pairing>::G2Affine> as Decode>::decode(
        &mut init_msg.g2_gen.as_slice(),
    )
    .unwrap();

    let mut pub_keys = Vec::new();
    let mut aggregate_pub_key: G2Affine = Default::default();

    for pub_key_bytes in init_msg.pub_keys.iter() {
        let pub_key = <ArkScale<<Bls12_381 as Pairing>::G2Affine> as Decode>::decode(
            &mut pub_key_bytes.as_slice(),
        )
        .unwrap();
        aggregate_pub_key = (aggregate_pub_key + pub_key.0).into();
        pub_keys.push(pub_key.0);
    }

    let contract = Contract {
        g2_gen: g2_gen.0,
        pub_keys,
        aggregate_pub_key,
        miller_out: (None, None),
    };

    unsafe { CONTRACT = Some(contract) }
}

#[gstd::async_main]
async fn main() {
    let msg: HandleMessage = msg::load().expect("Unable to decode `HandleMessage`");
    let contract = unsafe {
        static_mut!(CONTRACT)
            .as_mut()
            .expect("The contract is not initialized")
    };

    match msg {
        HandleMessage::MillerLoop {
            message,
            signatures,
        } => {
            let aggregate_pub_key: ArkScale<Vec<G2Affine>> =
                vec![contract.aggregate_pub_key].into();

            let request = Request::MultiMillerLoop {
                a: message,
                b: aggregate_pub_key.encode(),
            }
            .encode();
            let reply = msg::send_bytes_for_reply(BUILTIN_BLS381, &request, 0, 0)
                .expect("Failed to send message")
                .await
                .expect("Received error reply");

            let response = Response::decode(&mut reply.as_slice()).unwrap();
            let miller_out1 = match response {
                Response::MultiMillerLoop(v) => v,
                _ => unreachable!(),
            };

            let mut aggregate_signature: G1Affine = Default::default();
            for signature in signatures.iter() {
                let signature = <ArkScale<<Bls12_381 as Pairing>::G1Affine> as Decode>::decode(
                    &mut signature.as_slice(),
                )
                .unwrap();
                aggregate_signature = (aggregate_signature + signature.0).into();
            }
            let aggregate_signature: ArkScale<Vec<G1Affine>> = vec![aggregate_signature].into();
            let g2_gen: ArkScale<Vec<G2Affine>> = vec![contract.g2_gen].into();
            let request = Request::MultiMillerLoop {
                a: aggregate_signature.encode(),
                b: g2_gen.encode(),
            }
            .encode();
            let reply = msg::send_bytes_for_reply(BUILTIN_BLS381, &request, 0, 0)
                .expect("Failed to send message")
                .await
                .expect("Received error reply");
            let response = Response::decode(&mut reply.as_slice()).unwrap();
            let miller_out2 = match response {
                Response::MultiMillerLoop(v) => v,
                _ => unreachable!(),
            };

            contract.miller_out = (Some(miller_out1), Some(miller_out2));
        }

        HandleMessage::Exp => {
            if let (Some(miller_out1), Some(miller_out2)) = &contract.miller_out {
                let request = Request::FinalExponentiation {
                    f: miller_out1.clone(),
                }
                .encode();
                let reply = msg::send_bytes_for_reply(BUILTIN_BLS381, &request, 0, 0)
                    .expect("Failed to send message")
                    .await
                    .expect("Received error reply");
                let response = Response::decode(&mut reply.as_slice()).unwrap();
                let exp1 = match response {
                    Response::FinalExponentiation(v) => {
                        ArkScale::<<Bls12_381 as Pairing>::TargetField>::decode(&mut v.as_slice())
                            .unwrap()
                    }
                    _ => unreachable!(),
                };

                let request = Request::FinalExponentiation {
                    f: miller_out2.clone(),
                }
                .encode();
                let reply = msg::send_bytes_for_reply(BUILTIN_BLS381, &request, 0, 0)
                    .expect("Failed to send message")
                    .await
                    .expect("Received error reply");
                let response = Response::decode(&mut reply.as_slice()).unwrap();
                let exp2 = match response {
                    Response::FinalExponentiation(v) => {
                        ArkScale::<<Bls12_381 as Pairing>::TargetField>::decode(&mut v.as_slice())
                            .unwrap()
                    }
                    _ => unreachable!(),
                };

                assert_eq!(exp1.0, exp2.0);

                contract.miller_out = (None, None);
            }
        }
    }
}
