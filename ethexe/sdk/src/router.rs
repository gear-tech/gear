// This file is part of Gear.

// Copyright (C) 2026 Gear Technologies Inc.
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

use crate::VaraEthApi;
use alloy::rpc::types::TransactionReceipt;
use anyhow::Result;
use ethexe_common::{
    Address, Digest, ValidatorsVec,
    gear::{AggregatedPublicKey, CodeState, ComputationSettings, Timelines},
};
use ethexe_ethereum::{
    IntoBlockId,
    abi::IRouter::StorageView,
    router::{
        CodeValidationResult, Router as EthereumRouter, RouterEvents as EthereumRouterEvents,
        RouterQuery as EthereumRouterQuery,
    },
};
use ethexe_rpc::ProgramClient;
use gprimitives::{ActorId, CodeId, H256};

pub struct Router<'a> {
    pub(crate) api: &'a VaraEthApi,
    pub(crate) router_client: EthereumRouter,
    pub(crate) router_query_client: EthereumRouterQuery,
}

impl<'a> Router<'a> {
    pub fn events(&self) -> EthereumRouterEvents<'_> {
        self.router_query_client.events()
    }

    // TODO: move StorageView into ethexe-common and export

    pub async fn storage_view(&self) -> Result<StorageView> {
        self.router_query_client.storage_view().await
    }

    pub async fn storage_view_at(&self, id: impl IntoBlockId) -> Result<StorageView> {
        self.router_query_client.storage_view_at(id).await
    }

    pub async fn genesis_block_hash(&self) -> Result<H256> {
        self.router_query_client.genesis_block_hash().await
    }

    pub async fn genesis_timestamp(&self) -> Result<u64> {
        self.router_query_client.genesis_timestamp().await
    }

    pub async fn latest_committed_batch_hash(&self) -> Result<Digest> {
        self.router_query_client.latest_committed_batch_hash().await
    }

    pub async fn latest_committed_batch_timestamp(&self) -> Result<u64> {
        self.router_query_client
            .latest_committed_batch_timestamp()
            .await
    }

    pub async fn mirror_impl(&self) -> Result<Address> {
        self.router_query_client.mirror_impl().await
    }

    pub async fn wvara_address(&self) -> Result<Address> {
        self.router_query_client.wvara_address().await
    }

    pub async fn middleware_address(&self) -> Result<Address> {
        self.router_query_client.middleware_address().await
    }

    pub async fn validators_aggregated_public_key(&self) -> Result<AggregatedPublicKey> {
        self.router_query_client
            .validators_aggregated_public_key()
            .await
    }

    pub async fn validators_verifiable_secret_sharing_commitment(&self) -> Result<Vec<u8>> {
        self.router_query_client
            .validators_verifiable_secret_sharing_commitment()
            .await
    }

    pub async fn are_validators(
        &self,
        validators: impl IntoIterator<Item = Address>,
    ) -> Result<bool> {
        self.router_query_client.are_validators(validators).await
    }

    pub async fn is_validator(&self, validator: Address) -> Result<bool> {
        self.router_query_client.is_validator(validator).await
    }

    pub async fn signing_threshold_fraction(&self) -> Result<(u128, u128)> {
        self.router_query_client.signing_threshold_fraction().await
    }

    pub async fn validators(&self) -> Result<ValidatorsVec> {
        self.router_query_client.validators().await
    }

    pub async fn validators_at(&self, id: impl IntoBlockId) -> Result<ValidatorsVec> {
        self.router_query_client.validators_at(id).await
    }

    pub async fn validators_count(&self) -> Result<u64> {
        self.router_query_client.validators_count().await
    }

    pub async fn validators_threshold(&self) -> Result<u64> {
        self.router_query_client.validators_threshold().await
    }

    pub async fn compute_settings(&self) -> Result<ComputationSettings> {
        self.router_query_client.compute_settings().await
    }

    pub async fn code_state(&self, code_id: CodeId) -> Result<CodeState> {
        self.router_query_client.code_state(code_id).await
    }

    pub async fn codes_states(
        &self,
        code_ids: impl IntoIterator<Item = CodeId>,
    ) -> Result<Vec<CodeState>> {
        self.router_query_client.codes_states(code_ids).await
    }

    pub async fn codes_states_at(
        &self,
        code_ids: impl IntoIterator<Item = CodeId>,
        id: impl IntoBlockId,
    ) -> Result<Vec<CodeState>> {
        self.router_query_client.codes_states_at(code_ids, id).await
    }

    pub async fn program_ids(&self) -> Result<Vec<ActorId>> {
        let program_ids = self.api.vara_eth_client.ids().await?;
        Ok(program_ids.into_iter().map(ActorId::from).collect())
    }

    pub async fn program_code_id(&self, program_id: ActorId) -> Result<Option<CodeId>> {
        self.router_query_client.program_code_id(program_id).await
    }

    pub async fn programs_code_ids(
        &self,
        program_ids: impl IntoIterator<Item = ActorId>,
    ) -> Result<Vec<CodeId>> {
        self.router_query_client
            .programs_code_ids(program_ids)
            .await
    }

    pub async fn programs_code_ids_at(
        &self,
        program_ids: impl IntoIterator<Item = ActorId>,
        id: impl IntoBlockId,
    ) -> Result<Vec<CodeId>> {
        self.router_query_client
            .programs_code_ids_at(program_ids, id)
            .await
    }

    pub async fn programs_count(&self) -> Result<u64> {
        self.router_query_client.programs_count().await
    }

    pub async fn programs_count_at(&self, id: impl IntoBlockId) -> Result<u64> {
        self.router_query_client.programs_count_at(id).await
    }

    pub async fn validated_codes_count(&self) -> Result<u64> {
        self.router_query_client.validated_codes_count().await
    }

    pub async fn validated_codes_count_at(&self, id: impl IntoBlockId) -> Result<u64> {
        self.router_query_client.validated_codes_count_at(id).await
    }

    pub async fn timelines(&self) -> Result<Timelines> {
        self.router_query_client.timelines().await
    }

    pub async fn set_mirror(&self, new_mirror: Address) -> Result<H256> {
        self.router_client.set_mirror(new_mirror).await
    }

    pub async fn set_mirror_with_receipt(&self, new_mirror: Address) -> Result<TransactionReceipt> {
        self.router_client.set_mirror_with_receipt(new_mirror).await
    }

    pub async fn lookup_genesis_hash(&self) -> Result<H256> {
        self.router_client.lookup_genesis_hash().await
    }

    pub async fn lookup_genesis_hash_with_receipt(&self) -> Result<TransactionReceipt> {
        self.router_client.lookup_genesis_hash_with_receipt().await
    }

    pub async fn request_code_validation(&self, code: &[u8]) -> Result<(H256, CodeId)> {
        self.router_client.request_code_validation(code).await
    }

    pub async fn request_code_validation_with_receipt(
        &self,
        code: &[u8],
    ) -> Result<(TransactionReceipt, CodeId)> {
        self.router_client
            .request_code_validation_with_receipt(code)
            .await
    }

    pub async fn wait_for_code_validation(&self, code_id: CodeId) -> Result<CodeValidationResult> {
        self.router_client.wait_for_code_validation(code_id).await
    }

    pub async fn create_program(
        &self,
        code_id: CodeId,
        salt: H256,
        override_initializer: Option<ActorId>,
    ) -> Result<(H256, ActorId)> {
        self.router_client
            .create_program(code_id, salt, override_initializer)
            .await
    }

    pub async fn create_program_with_receipt(
        &self,
        code_id: CodeId,
        salt: H256,
        override_initializer: Option<ActorId>,
    ) -> Result<(TransactionReceipt, ActorId)> {
        self.router_client
            .create_program_with_receipt(code_id, salt, override_initializer)
            .await
    }

    pub async fn create_program_with_abi_interface(
        &self,
        code_id: CodeId,
        salt: H256,
        override_initializer: Option<ActorId>,
        abi_interface: ActorId,
    ) -> Result<(H256, ActorId)> {
        self.router_client
            .create_program_with_abi_interface(code_id, salt, override_initializer, abi_interface)
            .await
    }

    pub async fn create_program_with_abi_interface_with_receipt(
        &self,
        code_id: CodeId,
        salt: H256,
        override_initializer: Option<ActorId>,
        abi_interface: ActorId,
    ) -> Result<(TransactionReceipt, ActorId)> {
        self.router_client
            .create_program_with_abi_interface_with_receipt(
                code_id,
                salt,
                override_initializer,
                abi_interface,
            )
            .await
    }
}
