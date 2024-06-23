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

use crate::{Config, Error, Pallet};
use common::Origin;
use core::marker::PhantomData;
use gbuiltin_eth_bridge::{Request, Response};
use gear_core::{
    message::{Payload, StoredDispatch},
    str::LimitedStr,
};
use gprimitives::{ActorId, H160};
use pallet_gear_builtin::{BuiltinActor, BuiltinActorError};
use parity_scale_codec::{Decode, Encode};
use sp_std::vec::Vec;

/// Gear builtin actor providing functionality of `pallet-gear-eth-bridge`.
///
/// Check out `gbuiltin-eth-bridge` to observe builtin interface.
pub struct Actor<T: Config>(PhantomData<T>);

impl<T: Config> BuiltinActor for Actor<T>
where
    T::AccountId: Origin,
{
    const ID: u64 = 2;

    type Error = BuiltinActorError;

    fn handle(dispatch: &StoredDispatch, gas_limit: u64) -> (Result<Payload, Self::Error>, u64) {
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

// TODO (breathx): properly handle gas limit.
fn send_message_request<T: Config>(
    source: ActorId,
    destination: H160,
    payload: Vec<u8>,
    gas_limit: u64,
) -> (Result<Payload, BuiltinActorError>, u64)
where
    T::AccountId: Origin,
{
    let res = Pallet::<T>::queue_message(source, destination, payload)
        .map(|(nonce, hash)| {
            Response::EthMessageQueued { nonce, hash }
                .encode()
                .try_into()
                .unwrap_or_else(|_| unreachable!("response max encoded len is less than maximum"))
        })
        .map_err(|e| {
            let error_str = match e {
                Error::BridgeIsNotYetInitialized => "Send message: bridge is not yet initialized",
                Error::BridgeIsPaused => "Send message: bridge is paused",
                Error::MaxPayloadSizeExceeded => "Send message: message max payload size exceeded",
                Error::QueueCapacityExceeded => "Send message: queue capacity exceeded",
                _ => unimplemented!(),
            };

            BuiltinActorError::Custom(LimitedStr::from_small_str(error_str))
        });

    (res, gas_limit)
}
