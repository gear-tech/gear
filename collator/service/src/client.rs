// This file is part of Gear.

// Copyright (C) 2021-2022 Gear Technologies Inc.
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

pub use runtime_primitives::{
    AccountId, Balance, Block, BlockNumber, Hash, Header, Index as Nonce,
};
use sp_runtime::traits::BlakeTwo256;

#[cfg(not(any(feature = "rococo-gear-native",)))]
compile_error!("at least one runtime feature must be enabled");

/// A set of APIs that polkadot-like runtimes must implement.
///
/// This trait has no methods or associated type. It is a concise marker for all the trait bounds
/// that it contains.
#[cfg(feature = "rococo-gear-native")]
pub trait RuntimeApiCollection:
    sp_transaction_pool::runtime_api::TaggedTransactionQueue<Block>
    + sp_api::ApiExt<Block>
    + sp_block_builder::BlockBuilder<Block>
    + sp_api::Metadata<Block>
    + sp_offchain::OffchainWorkerApi<Block>
    + sp_session::SessionKeys<Block>
    + cumulus_primitives_core::CollectCollationInfo<Block>
    + pallet_transaction_payment_rpc::TransactionPaymentRuntimeApi<Block, Balance>
    + substrate_frame_rpc_system::AccountNonceApi<Block, AccountId, Nonce>
    + pallet_gear_rpc::GearRuntimeApi<Block>
where
    <Self as sp_api::ApiExt<Block>>::StateBackend: sp_api::StateBackend<BlakeTwo256>,
{
}

#[cfg(feature = "rococo-gear-native")]
impl<Api> RuntimeApiCollection for Api
where
    Api: sp_transaction_pool::runtime_api::TaggedTransactionQueue<Block>
        + sp_api::ApiExt<Block>
        + sp_block_builder::BlockBuilder<Block>
        + sp_api::Metadata<Block>
        + sp_offchain::OffchainWorkerApi<Block>
        + sp_session::SessionKeys<Block>
        + cumulus_primitives_core::CollectCollationInfo<Block>
        + pallet_transaction_payment_rpc::TransactionPaymentRuntimeApi<Block, Balance>
        + substrate_frame_rpc_system::AccountNonceApi<Block, AccountId, Nonce>
        + pallet_gear_rpc::GearRuntimeApi<Block>,
    <Self as sp_api::ApiExt<Block>>::StateBackend: sp_api::StateBackend<BlakeTwo256>,
{
}
