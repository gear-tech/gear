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

//! Proxy builtin actor implementation.

use super::*;
use common::Origin;
use gbuiltin_proxy::{ProxyType as BuiltinProxyType, Request};
use pallet_proxy::Config as ProxyConfig;
use sp_runtime::traits::StaticLookup;

/// Proxy builtin actor.
pub struct Actor<T: Config + ProxyConfig>(PhantomData<T>);

impl<T: Config + ProxyConfig> Actor<T>
where
    T::AccountId: Origin,
    <T as ProxyConfig>::ProxyType: From<BuiltinProxyType>,
    CallOf<T>: From<pallet_proxy::Call<T>>,
{
    /// Casts received request to a runtime call.
    fn cast(request: Request) -> Result<CallOf<T>, BuiltinActorError> {
        Ok(match request {
            Request::AddProxy {
                delegate,
                proxy_type,
            } => {
                let delegate = T::Lookup::unlookup(delegate.cast());
                let proxy_type = proxy_type.into();
                let delay = 0u32.into();

                pallet_proxy::Call::<T>::add_proxy {
                    delegate,
                    proxy_type,
                    delay,
                }
            }
            Request::RemoveProxy {
                delegate,
                proxy_type,
            } => {
                let delegate = T::Lookup::unlookup(delegate.cast());
                let proxy_type = proxy_type.into();
                let delay = 0u32.into();

                pallet_proxy::Call::<T>::remove_proxy {
                    delegate,
                    proxy_type,
                    delay,
                }
            }
        }
        .into())
    }
}

impl<T: Config + ProxyConfig> BuiltinActor for Actor<T>
where
    T::AccountId: Origin,
    <T as ProxyConfig>::ProxyType: From<BuiltinProxyType>,
    CallOf<T>: From<pallet_proxy::Call<T>>,
{
    fn handle(
        dispatch: &StoredDispatch,
        context: &mut BuiltinContext,
    ) -> Result<BuiltinReply, BuiltinActorError> {
        let request = Request::decode(&mut dispatch.payload_bytes())
            .map_err(|_| BuiltinActorError::DecodingError)?;

        let origin = dispatch.source();

        let call = Self::cast(request)?;

        Ok(BuiltinReply {
            payload: Pallet::<T>::dispatch_call(origin, call, context)
                .map(|_| Default::default())?,
            // The value is not used in the proxy actor, it will be fully returned to the caller.
            value: dispatch.value(),
        })
    }

    fn max_gas() -> u64 {
        Default::default()
    }
}
