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

use crate::{Config, Error, Pallet, WeightInfo};
use common::{storage::Limiter, BlockLimiter, Origin};
use core::marker::PhantomData;
use gbuiltin_eth_bridge::{Request, Response};
use gear_core::{
    message::{Payload, StoredDispatch},
    str::LimitedStr,
};
use gprimitives::{ActorId, H160};
use pallet_gear_builtin::{BuiltinActor, BuiltinActorError};
use parity_scale_codec::{Decode, Encode};
use sp_runtime::traits::Zero;
use sp_std::vec::Vec;

pub type GasAllowanceOf<T> = <<T as Config>::BlockLimiter as BlockLimiter>::GasAllowance;

/// Gear builtin actor providing functionality of `pallet-gear-eth-bridge`.
///
/// Check out `gbuiltin-eth-bridge` to observe builtin interface.
pub struct Actor<T: Config>(PhantomData<T>);

impl<T: Config> BuiltinActor for Actor<T>
where
    T::AccountId: Origin,
{
    fn handle(
        dispatch: &StoredDispatch,
        gas_limit: u64,
    ) -> (Result<Payload, BuiltinActorError>, u64) {
        if !dispatch.value().is_zero() {
            return (
                Err(BuiltinActorError::Custom(LimitedStr::from_small_str(
                    error_to_str(&Error::<T>::IncorrectValueApplied),
                ))),
                0,
            );
        }

        let Ok(request) = Request::decode(&mut dispatch.payload_bytes()) else {
            return (Err(BuiltinActorError::DecodingError), 0);
        };

        match request {
            Request::SendEthMessage {
                destination,
                payload,
            } => send_message_request::<T>(dispatch.source(), destination, payload, gas_limit),
        }
    }
}

fn send_message_request<T: Config>(
    source: ActorId,
    destination: H160,
    payload: Vec<u8>,
    gas_limit: u64,
) -> (Result<Payload, BuiltinActorError>, u64)
where
    T::AccountId: Origin,
{
    let gas_cost = <T as Config>::WeightInfo::send_eth_message().ref_time();

    if gas_limit < gas_cost {
        return (Err(BuiltinActorError::InsufficientGas), 0);
    }
    if GasAllowanceOf::<T>::get() < gas_cost {
        return (Err(BuiltinActorError::GasAllowanceExceeded), 0);
    }

    let res = Pallet::<T>::queue_message(source, destination, payload)
        .map(|(nonce, hash)| {
            Response::EthMessageQueued { nonce, hash }
                .encode()
                .try_into()
                .unwrap_or_else(|_| unreachable!("response max encoded len is less than maximum"))
        })
        .map_err(|e| BuiltinActorError::Custom(LimitedStr::from_small_str(error_to_str(&e))));

    (res, gas_cost)
}

pub fn error_to_str<T: Config>(error: &Error<T>) -> &'static str {
    match error {
        Error::BridgeIsNotYetInitialized => "Send message: bridge is not yet initialized",
        Error::BridgeIsPaused => "Send message: bridge is paused",
        Error::MaxPayloadSizeExceeded => "Send message: message max payload size exceeded",
        Error::QueueCapacityExceeded => "Send message: queue capacity exceeded",
        Error::IncorrectValueApplied => "Send message: incorrect value applied",
        _ => unimplemented!(),
    }
}
