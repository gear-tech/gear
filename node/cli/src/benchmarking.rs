// This file is part of Gear.

// Copyright (C) 2022 Gear Technologies Inc.
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

//! Setup code for [`super::command`] which would otherwise bloat that module.
//!
//! Should only be used for benchmarking as it may break in other contexts.

use service::Client;

use runtime_primitives::{AccountId, Signature};
use sc_cli::Result;
use sc_client_api::BlockBackend;
use sp_core::{Encode, Pair};
use sp_inherents::{InherentData, InherentDataProvider};
use sp_keyring::Sr25519Keyring;
use sp_runtime::OpaqueExtrinsic;

use std::{sync::Arc, time::Duration};

/// Provides a `SignedPayload` for any runtime.
///
/// Should only be used for benchmarking as it is not tested for regular usage.
///
/// The first code block should set up all variables that are needed to create the
/// `SignedPayload`. The second block can make use of the `SignedPayload`.
///
/// This is not done as a trait function since the return type depends on the runtime.
/// This macro therefore uses the same approach as [`with_client!`].
macro_rules! with_signed_payload {
    {
        $self:ident,
        {
            $extra:ident,
            $client:ident,
            $raw_payload:ident
        },
        {
            $( $setup:tt )*
        },
        (
            $period:expr,
            $current_block:expr,
            $nonce:expr,
            $call:expr,
            $genesis:expr,
            $best_hash:expr,
            $tip:expr
        ),
        {
            $( $usage:tt )*
        }
    } => {
        match $self.$client.as_ref() {
            #[cfg(feature = "gear-native")]
            Client::Gear($client) => {
                use gear_runtime as runtime;

                $( $setup )*
                let $extra: runtime::SignedExtra = (
                    frame_system::CheckNonZeroSender::<runtime::Runtime>::new(),
                    frame_system::CheckSpecVersion::<runtime::Runtime>::new(),
                    frame_system::CheckTxVersion::<runtime::Runtime>::new(),
                    frame_system::CheckGenesis::<runtime::Runtime>::new(),
                    frame_system::CheckMortality::<runtime::Runtime>::from(
                        sp_runtime::generic::Era::mortal($period, $current_block),
                    ),
                    frame_system::CheckNonce::<runtime::Runtime>::from($nonce),
                    frame_system::CheckWeight::<runtime::Runtime>::new(),
                    pallet_gear_payment::CustomChargeTransactionPayment::<runtime::Runtime>::from($tip),
                );

                let $raw_payload = runtime::SignedPayload::from_raw(
                    $call.clone(),
                    $extra.clone(),
                    (
                        (),
                        runtime::VERSION.spec_version,
                        runtime::VERSION.transaction_version,
                        $genesis,
                        $best_hash,
                        (),
                        (),
                        (),
                    ),
                );

                $( $usage )*
            },
            #[cfg(feature = "vara-native")]
            Client::Vara($client) => {
                use vara_runtime as runtime;

                $( $setup )*

                let $extra: runtime::SignedExtra = (
                    runtime::DisableValueTransfers,
                    pallet_gear_staking_rewards::StakingBlackList::<runtime::Runtime>::new(),
                    frame_system::CheckNonZeroSender::<runtime::Runtime>::new(),
                    frame_system::CheckSpecVersion::<runtime::Runtime>::new(),
                    frame_system::CheckTxVersion::<runtime::Runtime>::new(),
                    frame_system::CheckGenesis::<runtime::Runtime>::new(),
                    frame_system::CheckMortality::<runtime::Runtime>::from(
                        sp_runtime::generic::Era::mortal($period, $current_block),
                    ),
                    frame_system::CheckNonce::<runtime::Runtime>::from($nonce),
                    frame_system::CheckWeight::<runtime::Runtime>::new(),
                    pallet_gear_payment::CustomChargeTransactionPayment::<runtime::Runtime>::from($tip),
                );

                let $raw_payload = runtime::SignedPayload::from_raw(
                    $call.clone(),
                    $extra.clone(),
                    (
                        (),
                        (),
                        (),
                        runtime::VERSION.spec_version,
                        runtime::VERSION.transaction_version,
                        $genesis,
                        $best_hash,
                        (),
                        (),
                        (),
                    ),
                );

                $( $usage )*
            },
        }
    }
}

/// Generates extrinsics for the `benchmark overhead` command.
///
/// Note: Should only be used for benchmarking.
pub struct RemarkBuilder {
    client: Arc<Client>,
}

impl RemarkBuilder {
    /// Creates a new [`Self`] from the given client.
    pub fn new(client: Arc<Client>) -> Self {
        Self { client }
    }
}

impl frame_benchmarking_cli::ExtrinsicBuilder for RemarkBuilder {
    fn pallet(&self) -> &str {
        "system"
    }

    fn extrinsic(&self) -> &str {
        "remark"
    }

    fn build(&self, nonce: u32) -> std::result::Result<OpaqueExtrinsic, &'static str> {
        with_signed_payload! {
            self,
            {extra, client, raw_payload},
            {
                // First the setup code to init all the variables that are needed
                // to build the signed extras.
                use runtime::{RuntimeCall, SystemCall};

                let genesis_hash = client
                    .block_hash(0)
                    .ok()
                    .flatten()
                    .expect("Genesis block exists; qed");
                let call = RuntimeCall::System(SystemCall::remark { remark: vec![] });
                let bob = Sr25519Keyring::Bob.pair();
                let period = runtime::BlockHashCount::get()
                    .checked_next_power_of_two()
                    .map(|c| c / 2)
                    .unwrap_or(2) as u64;
                let best_block = client.chain_info().best_number;
                let best_hash = client.chain_info().best_hash;
                let tip = 0;
            },
            (period, best_block.into(), nonce, call, genesis_hash, best_hash, tip),
            /* The SignedPayload is generated here */
            {
                // Use the payload to generate a signature.
                let signature = raw_payload.using_encoded(|payload| bob.sign(payload));

                let ext = runtime::UncheckedExtrinsic::new_signed(
                    call,
                    sp_runtime::AccountId32::from(bob.public()).into(),
                    runtime_primitives::Signature::Sr25519(signature),
                    extra,
                );
                Ok(ext.into())
            }
        }
    }
}

/// Generates `Balances::TransferKeepAlive` extrinsics for the benchmarks.
///
/// Note: Should only be used for benchmarking.
pub struct TransferKeepAliveBuilder {
    client: Arc<Client>,
    dest: AccountId,
}

impl TransferKeepAliveBuilder {
    /// Creates a new [`Self`] from the given client.
    //
    // Note: In current implementation the `value` (whose meaning is existential deposit)
    // is not passed as a parameter because it is concrete runtime dependent while the
    // caller of this method doesn't know which concrete runtime is being used.
    // Hardcoding it to the `runtime::EXISTENTIAL_DEPOSIT` for the meantime until
    // we find a better solution.
    pub fn new(client: Arc<Client>, dest: AccountId) -> Self {
        Self { client, dest }
    }
}

impl frame_benchmarking_cli::ExtrinsicBuilder for TransferKeepAliveBuilder {
    fn pallet(&self) -> &str {
        "balances"
    }

    fn extrinsic(&self) -> &str {
        "transfer_keep_alive"
    }

    fn build(&self, nonce: u32) -> std::result::Result<OpaqueExtrinsic, &'static str> {
        with_signed_payload! {
            self,
            {extra, client, raw_payload},
            {
                // First the setup code to init all the variables that are needed
                // to build the signed extras.
                use runtime::{RuntimeCall, BalancesCall, EXISTENTIAL_DEPOSIT};

                let genesis_hash = client
                    .block_hash(0)
                    .ok()
                    .flatten()
                    .expect("Genesis block exists; qed");
                let call = RuntimeCall::Balances(BalancesCall::transfer_keep_alive {
                    dest: self.dest.clone().into(),
                    value: EXISTENTIAL_DEPOSIT,
                });
                let bob = Sr25519Keyring::Bob.pair();
                let period = runtime::BlockHashCount::get()
                    .checked_next_power_of_two()
                    .map(|c| c / 2)
                    .unwrap_or(2) as u64;
                let best_block = client.chain_info().best_number;
                let best_hash = client.chain_info().best_hash;
                let tip = 0;
            },
            (period, best_block.into(), nonce, call, genesis_hash, best_hash, tip),
            /* The SignedPayload is generated here */
            {
                // Use the payload to generate a signature.
                let signature = raw_payload.using_encoded(|payload| bob.sign(payload));

                let ext = runtime::UncheckedExtrinsic::new_signed(
                    call,
                    sp_runtime::AccountId32::from(bob.public()).into(),
                    Signature::Sr25519(signature),
                    extra,
                );
                Ok(ext.into())
            }
        }
    }
}

/// Generates inherent data for the `benchmark overhead` command.
pub fn inherent_benchmark_data() -> Result<InherentData> {
    let mut inherent_data = InherentData::new();
    let d = Duration::from_millis(0);
    let timestamp = sp_timestamp::InherentDataProvider::new(d.into());

    futures::executor::block_on(timestamp.provide_inherent_data(&mut inherent_data))
        .map_err(|e| format!("creating inherent data: {e:?}"))?;
    Ok(inherent_data)
}
