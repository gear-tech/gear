// This file is part of Gear.

// Copyright (C) 2021-2025 Gear Technologies Inc.
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

//! Webpki builtin actor implementation

use super::*;
use core::marker::PhantomData;
use gbuiltin_webpki::{Request, Response};
use parity_scale_codec::Decode;

pub struct Actor<T: Config>(PhantomData<T>);

impl<T: Config> BuiltinActor for Actor<T> {
    fn handle(
        dispatch: &StoredDispatch,
        context: &mut BuiltinContext,
    ) -> Result<BuiltinReply, BuiltinActorError> {
        let message = dispatch.message();
        let mut payload = message.payload_bytes();

        let request =
            Request::decode(&mut payload).map_err(|_| BuiltinActorError::DecodingError)?;

        match request {
            Request::VerifyCertsChain {
                ders,
                sni,
                timestamp,
            } => verify_certs_chain::<T>(&ders, &sni, timestamp, context),
            Request::VerifySignature {
                der,
                message,
                signature,
                algo,
            } => verify_signature::<T>(&der, &message, &signature, algo, context),
        }
        .map(|response| BuiltinReply {
            payload: response.encode().try_into().unwrap_or_else(|err| {
                let err_msg = format!(
                    "Actor::handle: Response message is too large. \
                        Response - {response:X?}. Got error - {err:?}"
                );

                log::error!("{err_msg}");
                unreachable!("{err_msg}")
            }),
            // The value is not used in the bls12_381 actor, it will be fully returned to the caller.
            value: dispatch.value(),
        })
    }

    fn max_gas() -> u64 {
        Default::default()
    }
}

fn verify_certs_chain<T: Config>(
    ders: &[Vec<u8>],
    sni: &[u8],
    timestamp: u64,
    context: &mut BuiltinContext,
) -> Result<Response, BuiltinActorError> {
    let to_spend = <T as Config>::WeightInfo::webpki_verify_cert_chain().ref_time();
    context.try_charge_gas(to_spend)?;

    gear_runtime_interface::gear_webpki::verify_certs_chain(ders, sni, timestamp)
        .map(|(cert_valid, dns_valid)| Response::VerifyCertsChain {
            certs_chain_ok: cert_valid,
            dns_ok: dns_valid,
        })
        .map_err(|e| {
            log::debug!(
                target: LOG_TARGET,
                "Failed to verify certs chain: {e}"
            );

            BuiltinActorError::Custom(LimitedStr::from_small_str("Verify Certs Chain error"))
        })
}

fn verify_signature<T: Config>(
    der: &[u8],
    message: &[u8],
    signature: &[u8],
    algo: u16,
    context: &mut BuiltinContext,
) -> Result<Response, BuiltinActorError> {
    let to_spend = <T as Config>::WeightInfo::webpki_verify_signature().ref_time();
    context.try_charge_gas(to_spend)?;

    gear_runtime_interface::gear_webpki::verify_signature(der, message, signature, algo)
        .map(|sig_valid| Response::VerifySignature {
            signature_ok: sig_valid,
        })
        .map_err(|e| {
            log::debug!(
                target: LOG_TARGET,
                "Failed to verify signature: {e}"
            );

            BuiltinActorError::Custom(LimitedStr::from_small_str("Verify Signature error"))
        })
}
