// This file is part of Gear.

// Copyright (C) 2024 Gear Technologies Inc.
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
use core::marker::PhantomData;
use parity_scale_codec::{Compact, Input};

pub struct Actor<T: Config>(PhantomData<T>);

impl<T: Config> BuiltinActor for Actor<T> {
    const ID: u64 = 2;

    type Error = BuiltinActorError;

    fn handle(dispatch: &StoredDispatch, gas_limit: u64) -> (Result<Payload, Self::Error>, u64) {
        let message = dispatch.message();
        // Should be scale-encoded (common_curcuit_data: Vec<u8>, verifier_circuit_data: Vec<u8>, proof: Vec<u8>).
        let mut payload = message.payload_bytes();

        let (gas_spent, result) = decode_vec::<T, _>(gas_limit, 0, &mut payload);
        let common_circuit_data = match result {
            Some(Ok(array)) => array,
            Some(Err(e)) => return (Err(e), gas_spent),
            None => return (Err(BuiltinActorError::InsufficientGas), gas_spent),
        };

        let (gas_spent, result) = decode_vec::<T, _>(gas_limit, gas_spent, &mut payload);
        let verifier_circuit_data = match result {
            Some(Ok(array)) => array,
            Some(Err(e)) => return (Err(e), gas_spent),
            None => return (Err(BuiltinActorError::InsufficientGas), gas_spent),
        };

        let (mut gas_spent, result) = decode_vec::<T, _>(gas_limit, gas_spent, &mut payload);
        let proof = match result {
            Some(Ok(array)) => array,
            Some(Err(e)) => return (Err(e), gas_spent),
            None => return (Err(BuiltinActorError::InsufficientGas), gas_spent),
        };

        // TODO: depends on parameters.
        let to_spend = <T as Config>::WeightInfo::plonky2_decode().ref_time();
        if gas_limit < gas_spent + to_spend {
            return (Err(BuiltinActorError::InsufficientGas), gas_spent);
        }
    
        gas_spent += to_spend;

        let Ok(encoded) = gear_runtime_interface::specific_plonky_2::decode(common_circuit_data.clone(), proof.clone()) else {
            return (
                Err(BuiltinActorError::Custom(LimitedStr::from_small_str(
                    "specific_plonky_2: re-encode error",
                ))),
                gas_spent,
            );
        };

        let to_spend = <T as Config>::WeightInfo::decode_benchmark_data().ref_time();
        if gas_limit < gas_spent + to_spend {
            return (Err(BuiltinActorError::InsufficientGas), gas_spent);
        }
    
        gas_spent += to_spend;

        let Ok((circuit_config, public_input_count)) = <(gear_runtime_interface::CircuitConfig, u32)>::decode(&mut &encoded[..]) else {
            return (Err(BuiltinActorError::DecodingError), gas_spent);
        };

        let to_spend = <T as Config>::WeightInfo::plonky2_verify(public_input_count, circuit_config.fri_config.num_query_rounds).ref_time();
        if gas_limit < gas_spent + to_spend {
            return (Err(BuiltinActorError::InsufficientGas), gas_spent);
        }
    
        gas_spent += to_spend;

        let verify_result = gear_runtime_interface::specific_plonky_2::verify(common_circuit_data, verifier_circuit_data, proof);
        if verify_result == u32::from(gear_runtime_interface::Plonky2VerifyResult::Verified) {
            (Ok(Default::default()), gas_spent)
        } else {
            log::debug!("specific_plonky_2::verify: {verify_result}");

            (Err(BuiltinActorError::Custom(LimitedStr::from_small_str(
                "specific_plonky_2: verification error",
            ))), gas_spent)
        }
    }
}

fn decode_vec<T: Config, I: Input>(
    gas_limit: u64,
    mut gas_spent: u64,
    input: &mut I,
) -> (u64, Option<Result<Vec<u8>, BuiltinActorError>>) {
    let Ok(len) = Compact::<u32>::decode(input).map(u32::from) else {
        log::debug!(
            target: LOG_TARGET,
            "Failed to scale-decode vector length"
        );
        return (gas_spent, Some(Err(BuiltinActorError::DecodingError)));
    };

    let to_spend = <T as Config>::WeightInfo::decode_bytes(len).ref_time();
    if gas_limit < gas_spent + to_spend {
        return (gas_spent, None);
    }

    gas_spent += to_spend;

    let mut items = vec![0u8; len as usize];
    let bytes_slice = items.as_mut_slice();
    let result = input.read(bytes_slice).map(|_| items).map_err(|_| {
        log::debug!(
            target: LOG_TARGET,
            "Failed to scale-decode vector data",
        );

        BuiltinActorError::DecodingError
    });

    (gas_spent, Some(result))
}
