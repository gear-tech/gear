// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

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

/// Scoped handle for interacting with the on-chain Router contract and its associated queries.
///
/// Obtained via [`VaraEthApi::router`]. Write methods delegate to `EthereumRouter` (state-changing
/// Ethereum transactions); read methods delegate to `EthereumRouterQuery` (view calls). The handle
/// borrows the parent [`VaraEthApi`] and cannot outlive it.
pub struct Router<'a> {
    pub(crate) api: &'a VaraEthApi,
    pub(crate) router_client: EthereumRouter,
    pub(crate) router_query_client: EthereumRouterQuery,
}

impl<'a> Router<'a> {
    /// Returns a handle for subscribing to and querying Router contract events.
    pub fn events(&self) -> EthereumRouterEvents<'_> {
        self.router_query_client.events()
    }

    // TODO: move StorageView into ethexe-common and export

    /// Returns a snapshot of all Router contract storage fields at the latest block.
    pub async fn storage_view(&self) -> Result<StorageView> {
        self.router_query_client.storage_view().await
    }

    /// Returns a snapshot of all Router contract storage fields at the specified block.
    pub async fn storage_view_at(&self, id: impl IntoBlockId) -> Result<StorageView> {
        self.router_query_client.storage_view_at(id).await
    }

    /// Returns the Ethereum block hash that was recorded as the ethexe genesis block.
    pub async fn genesis_block_hash(&self) -> Result<H256> {
        self.router_query_client.genesis_block_hash().await
    }

    /// Returns the Unix timestamp (seconds) of the ethexe genesis block.
    pub async fn genesis_timestamp(&self) -> Result<u64> {
        self.router_query_client.genesis_timestamp().await
    }

    /// Returns the digest of the most recently committed batch.
    pub async fn latest_committed_batch_hash(&self) -> Result<Digest> {
        self.router_query_client.latest_committed_batch_hash().await
    }

    /// Returns the Unix timestamp (seconds) of the most recently committed batch.
    pub async fn latest_committed_batch_timestamp(&self) -> Result<u64> {
        self.router_query_client
            .latest_committed_batch_timestamp()
            .await
    }

    /// Returns the address of the Mirror implementation contract used for program proxies.
    pub async fn mirror_impl(&self) -> Result<Address> {
        self.router_query_client.mirror_impl().await
    }

    /// Returns the address of the WrappedVara ERC-20 contract.
    pub async fn wvara_address(&self) -> Result<Address> {
        self.router_query_client.wvara_address().await
    }

    /// Returns the address of the Middleware contract responsible for validator management.
    pub async fn middleware_address(&self) -> Result<Address> {
        self.router_query_client.middleware_address().await
    }

    /// Returns the FROST aggregated public key of the current validator set.
    pub async fn validators_aggregated_public_key(&self) -> Result<AggregatedPublicKey> {
        self.router_query_client
            .validators_aggregated_public_key()
            .await
    }

    /// Returns the verifiable secret sharing (VSS) commitment bytes for the current validator set.
    pub async fn validators_verifiable_secret_sharing_commitment(&self) -> Result<Vec<u8>> {
        self.router_query_client
            .validators_verifiable_secret_sharing_commitment()
            .await
    }

    /// Returns `true` if every address in `validators` is a member of the current validator set.
    pub async fn are_validators(
        &self,
        validators: impl IntoIterator<Item = Address>,
    ) -> Result<bool> {
        self.router_query_client.are_validators(validators).await
    }

    /// Returns `true` if the given address is a member of the current validator set.
    pub async fn is_validator(&self, validator: Address) -> Result<bool> {
        self.router_query_client.is_validator(validator).await
    }

    /// Returns the signing threshold as a `(numerator, denominator)` fraction.
    pub async fn signing_threshold_fraction(&self) -> Result<(u128, u128)> {
        self.router_query_client.signing_threshold_fraction().await
    }

    /// Returns the ordered list of current validator addresses.
    pub async fn validators(&self) -> Result<ValidatorsVec> {
        self.router_query_client.validators().await
    }

    /// Returns the ordered list of validator addresses at the specified block.
    pub async fn validators_at(&self, id: impl IntoBlockId) -> Result<ValidatorsVec> {
        self.router_query_client.validators_at(id).await
    }

    /// Returns the total number of validators in the current set.
    pub async fn validators_count(&self) -> Result<u64> {
        self.router_query_client.validators_count().await
    }

    /// Returns the minimum number of validator signatures required to commit a batch.
    pub async fn validators_threshold(&self) -> Result<u64> {
        self.router_query_client.validators_threshold().await
    }

    /// Returns the current computation settings (gas limits, block parameters, etc.) from the Router.
    pub async fn compute_settings(&self) -> Result<ComputationSettings> {
        self.router_query_client.compute_settings().await
    }

    /// Returns the validation state of a single code blob identified by `code_id`.
    pub async fn code_state(&self, code_id: CodeId) -> Result<CodeState> {
        self.router_query_client.code_state(code_id).await
    }

    /// Returns the validation states for a collection of code blobs at the latest block.
    pub async fn codes_states(
        &self,
        code_ids: impl IntoIterator<Item = CodeId>,
    ) -> Result<Vec<CodeState>> {
        self.router_query_client.codes_states(code_ids).await
    }

    /// Returns the validation states for a collection of code blobs at the specified block.
    pub async fn codes_states_at(
        &self,
        code_ids: impl IntoIterator<Item = CodeId>,
        id: impl IntoBlockId,
    ) -> Result<Vec<CodeState>> {
        self.router_query_client.codes_states_at(code_ids, id).await
    }

    /// Returns the actor IDs of all programs known to the ethexe RPC node.
    pub async fn program_ids(&self) -> Result<Vec<ActorId>> {
        let program_ids = self.api.vara_eth_client.ids().await?;
        Ok(program_ids.into_iter().map(ActorId::from).collect())
    }

    /// Returns the code ID associated with `program_id`, or `None` if the program is not registered.
    pub async fn program_code_id(&self, program_id: ActorId) -> Result<Option<CodeId>> {
        self.router_query_client.program_code_id(program_id).await
    }

    /// Returns the code IDs for a collection of programs at the latest block.
    pub async fn programs_code_ids(
        &self,
        program_ids: impl IntoIterator<Item = ActorId>,
    ) -> Result<Vec<CodeId>> {
        self.router_query_client
            .programs_code_ids(program_ids)
            .await
    }

    /// Returns the code IDs for a collection of programs at the specified block.
    pub async fn programs_code_ids_at(
        &self,
        program_ids: impl IntoIterator<Item = ActorId>,
        id: impl IntoBlockId,
    ) -> Result<Vec<CodeId>> {
        self.router_query_client
            .programs_code_ids_at(program_ids, id)
            .await
    }

    /// Returns the total number of programs registered in the Router at the latest block.
    pub async fn programs_count(&self) -> Result<u64> {
        self.router_query_client.programs_count().await
    }

    /// Returns the total number of programs registered in the Router at the specified block.
    pub async fn programs_count_at(&self, id: impl IntoBlockId) -> Result<u64> {
        self.router_query_client.programs_count_at(id).await
    }

    /// Returns the number of code blobs that have passed validation at the latest block.
    pub async fn validated_codes_count(&self) -> Result<u64> {
        self.router_query_client.validated_codes_count().await
    }

    /// Returns the number of code blobs that had passed validation at the specified block.
    pub async fn validated_codes_count_at(&self, id: impl IntoBlockId) -> Result<u64> {
        self.router_query_client.validated_codes_count_at(id).await
    }

    /// Returns the current era timeline configuration from the Router contract.
    pub async fn timelines(&self) -> Result<Timelines> {
        self.router_query_client.timelines().await
    }

    /// Submits a transaction to update the Mirror implementation address on the Router.
    ///
    /// Returns the transaction hash. Use [`set_mirror_with_receipt`] to wait for inclusion.
    pub async fn set_mirror(&self, new_mirror: Address) -> Result<H256> {
        self.router_client.set_mirror(new_mirror).await
    }

    /// Submits a transaction to update the Mirror implementation address and waits for the receipt.
    pub async fn set_mirror_with_receipt(&self, new_mirror: Address) -> Result<TransactionReceipt> {
        self.router_client.set_mirror_with_receipt(new_mirror).await
    }

    /// Triggers a genesis-hash lookup on the Router and returns the transaction hash.
    pub async fn lookup_genesis_hash(&self) -> Result<H256> {
        self.router_client.lookup_genesis_hash().await
    }

    /// Triggers a genesis-hash lookup on the Router and waits for the transaction receipt.
    pub async fn lookup_genesis_hash_with_receipt(&self) -> Result<TransactionReceipt> {
        self.router_client.lookup_genesis_hash_with_receipt().await
    }

    /// Uploads a WASM code blob and requests validator validation, returning `(tx_hash, code_id)`.
    ///
    /// The returned `CodeId` is deterministic (blake2 hash of the code). Use
    /// [`wait_for_code_validation`] to block until validation completes.
    pub async fn request_code_validation(&self, code: &[u8]) -> Result<(H256, CodeId)> {
        self.router_client.request_code_validation(code).await
    }

    /// Uploads a WASM code blob and requests validator validation, returning `(receipt, code_id)`.
    pub async fn request_code_validation_with_receipt(
        &self,
        code: &[u8],
    ) -> Result<(TransactionReceipt, CodeId)> {
        self.router_client
            .request_code_validation_with_receipt(code)
            .await
    }

    /// Polls until the validators reach a decision on `code_id` and returns the result.
    pub async fn wait_for_code_validation(&self, code_id: CodeId) -> Result<CodeValidationResult> {
        self.router_client.wait_for_code_validation(code_id).await
    }

    /// Deploys a program from a validated code blob, returning `(tx_hash, actor_id)`.
    ///
    /// `salt` is mixed into the deterministic `ActorId` derivation. `override_initializer` replaces
    /// the default initializer actor when set.
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

    /// Deploys a program from a validated code blob and waits for the receipt, returning
    /// `(receipt, actor_id)`.
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

    /// Deploys a program and seeds it with `initial_executable_balance` WVara, returning
    /// `(tx_hash, actor_id)`.
    pub async fn create_program_with_executable_balance(
        &self,
        code_id: CodeId,
        salt: H256,
        override_initializer: Option<ActorId>,
        initial_executable_balance: u128,
    ) -> Result<(H256, ActorId)> {
        self.router_client
            .create_program_with_executable_balance(
                code_id,
                salt,
                override_initializer,
                initial_executable_balance,
            )
            .await
    }

    /// Deploys a program with an initial executable balance and waits for the receipt, returning
    /// `(receipt, actor_id)`.
    pub async fn create_program_with_executable_balance_and_receipt(
        &self,
        code_id: CodeId,
        salt: H256,
        override_initializer: Option<ActorId>,
        initial_executable_balance: u128,
    ) -> Result<(TransactionReceipt, ActorId)> {
        self.router_client
            .create_program_with_executable_balance_and_receipt(
                code_id,
                salt,
                override_initializer,
                initial_executable_balance,
            )
            .await
    }

    /// Deploys a program and associates it with an ABI interface actor, returning `(tx_hash, actor_id)`.
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

    /// Deploys a program with an ABI interface actor and waits for the receipt, returning
    /// `(receipt, actor_id)`.
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

    /// Deploys a program with an ABI interface actor and an initial executable balance, returning
    /// `(tx_hash, actor_id)`.
    pub async fn create_program_with_abi_interface_and_executable_balance(
        &self,
        code_id: CodeId,
        salt: H256,
        override_initializer: Option<ActorId>,
        abi_interface: ActorId,
        initial_executable_balance: u128,
    ) -> Result<(H256, ActorId)> {
        self.router_client
            .create_program_with_abi_interface_and_executable_balance(
                code_id,
                salt,
                override_initializer,
                abi_interface,
                initial_executable_balance,
            )
            .await
    }

    /// Deploys a program with an ABI interface actor and an initial executable balance, waits for
    /// the receipt, and returns `(receipt, actor_id)`.
    pub async fn create_program_with_abi_interface_and_executable_balance_with_receipt(
        &self,
        code_id: CodeId,
        salt: H256,
        override_initializer: Option<ActorId>,
        abi_interface: ActorId,
        initial_executable_balance: u128,
    ) -> Result<(TransactionReceipt, ActorId)> {
        self.router_client
            .create_program_with_abi_interface_and_executable_balance_with_receipt(
                code_id,
                salt,
                override_initializer,
                abi_interface,
                initial_executable_balance,
            )
            .await
    }
}
