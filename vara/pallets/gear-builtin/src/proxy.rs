// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

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
