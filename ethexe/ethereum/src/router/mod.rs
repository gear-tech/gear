// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use crate::{
    AlloyEthereum, AlloyProvider, Eip712PermitData, Ethereum, IntoBlockId, Sender, TryGetReceipt,
    abi::{
        GearLib, IRouter,
        utils::{uint48_to_u64, uint256_to_u256},
    },
    router::events::AllEventsBuilder,
    wvara::WVara,
};
use alloy::{
    consensus::{SidecarBuilder, SimpleCoder, constants::GWEI_TO_WEI},
    eips::BlockId,
    hex,
    primitives::{Address as AlloyAddress, Bytes, fixed_bytes},
    providers::{
        PendingTransactionBuilder, Provider, ProviderBuilder, RootProvider,
        utils::{Eip1559Estimation, Eip1559Estimator},
    },
    rpc::types::{TransactionReceipt, eth::state::AccountOverride},
};
use anyhow::{Result, anyhow};
use ethexe_common::{
    Address, Digest, ValidatorsVec,
    ecdsa::ContractSignature,
    events::router::CodeGotValidatedEvent,
    gear::{
        AggregatedPublicKey, BatchCommitment, CodeState, ComputationSettings, SignatureType,
        Timelines,
    },
};
use events::{
    BatchCommittedEventBuilder, CodeGotValidatedEventBuilder, CodeValidationRequestedEventBuilder,
    ComputationSettingsChangedEventBuilder, EBCommittedEventBuilder, MBCommittedEventBuilder,
    ProgramCreatedEventBuilder, StorageSlotChangedEventBuilder,
    ValidatorsCommittedForEraEventBuilder, signatures,
};
use futures::StreamExt;
use gear_core::ids::prelude::CodeIdExt as _;
use gprimitives::{ActorId, CodeId, H256};
use serde::Serialize;
use std::collections::HashMap;

/// Event type definitions and builder helpers for the Router contract.
pub mod events;

type Instance = IRouter::IRouterInstance<AlloyProvider>;
type QueryInstance = IRouter::IRouterInstance<RootProvider>;

/// Writable handle to the on-chain `IRouter` contract.
///
/// Holds an authenticated `Sender` and an EIP-1559 fee estimator so callers can
/// submit state-mutating transactions (code validation, program creation, batch
/// commitment) without managing connection details directly.
#[derive(Clone)]
pub struct Router {
    instance: Instance,
    wvara_address: AlloyAddress,
    eip1559_estimator: Eip1559Estimator,
    eip1559_max_fee_per_gas_in_gwei: u128,
    sender: Sender,
}

impl Router {
    /// `Gear.blockIsPredecessor(hash)` can consume up to 30_000 gas
    const GEAR_BLOCK_IS_PREDECESSOR_GAS: u64 = 30_000;
    /// Transaction gas limit cap
    const TX_GAS_LIMIT_CAP: u64 = 10_000_000;

    pub(crate) fn new(
        address: AlloyAddress,
        wvara_address: AlloyAddress,
        eip1559_estimator: Eip1559Estimator,
        eip1559_max_fee_per_gas_in_gwei: u128,
        sender: Sender,
        provider: AlloyProvider,
    ) -> Self {
        Self {
            instance: Instance::new(address, provider),
            wvara_address,
            eip1559_estimator,
            eip1559_max_fee_per_gas_in_gwei,
            sender,
        }
    }

    /// Returns the Ethereum address of the deployed Router contract.
    pub fn address(&self) -> Address {
        Address(*self.instance.address().0)
    }

    /// Returns a read-only [`RouterQuery`] bound to the same contract address and underlying provider.
    pub fn query(&self) -> RouterQuery {
        RouterQuery {
            instance: QueryInstance::new(
                *self.instance.address(),
                self.instance.provider().root().clone(),
            ),
        }
    }

    /// Returns a [`WVara`] handle pointing at the Wrapped Vara token contract associated with this router.
    pub fn wvara(&self) -> WVara {
        WVara::new(self.wvara_address, self.instance.provider().clone())
    }

    /// Updates the Mirror implementation address in the Router contract and returns the transaction hash.
    pub async fn set_mirror(&self, new_mirror: Address) -> Result<H256> {
        self.set_mirror_with_receipt(new_mirror)
            .await
            .map(|receipt| (*receipt.transaction_hash).into())
    }

    /// Updates the Mirror implementation address in the Router contract and returns the full transaction receipt.
    pub async fn set_mirror_with_receipt(&self, new_mirror: Address) -> Result<TransactionReceipt> {
        let new_mirror = AlloyAddress::new(new_mirror.0);
        let builder = self.instance.setMirror(new_mirror);
        let receipt = builder
            .send()
            .await?
            .try_get_receipt_check_reverted()
            .await?;
        Ok(receipt)
    }

    /// Calls the contract's `reinitialize` function and returns the transaction receipt.
    pub async fn reinitialize(&self) -> Result<TransactionReceipt> {
        let builder = self.instance.reinitialize();
        let receipt = builder
            .send()
            .await?
            .try_get_receipt_check_reverted()
            .await?;
        Ok(receipt)
    }

    /// Looks up and records the genesis block hash in the Router contract, returning the transaction hash.
    pub async fn lookup_genesis_hash(&self) -> Result<H256> {
        self.lookup_genesis_hash_with_receipt()
            .await
            .map(|receipt| (*receipt.transaction_hash).into())
    }

    /// Looks up and records the genesis block hash in the Router contract, returning the full transaction receipt.
    pub async fn lookup_genesis_hash_with_receipt(&self) -> Result<TransactionReceipt> {
        let builder = self.instance.lookupGenesisHash();
        let receipt = builder
            .send()
            .await?
            .try_get_receipt_check_reverted()
            .await?;
        Ok(receipt)
    }

    /// Submits a code blob for validation via EIP-7594 blob-sidecar transaction, returning the transaction hash and derived `CodeId`.
    pub async fn request_code_validation(&self, code: &[u8]) -> Result<(H256, CodeId)> {
        self.request_code_validation_with_receipt(code)
            .await
            .map(|(receipt, code_id)| ((*receipt.transaction_hash).into(), code_id))
    }

    /// Submits a code blob for validation via EIP-7594 blob-sidecar transaction, returning the full receipt and derived `CodeId`.
    ///
    /// Prepares an EIP-712 permit for the required base fee before sending.
    pub async fn request_code_validation_with_receipt(
        &self,
        code: &[u8],
    ) -> Result<(TransactionReceipt, CodeId)> {
        let Eip712PermitData { deadline, v, r, s } = Ethereum::prepare_permit_data(
            self.instance.provider(),
            self.wvara().query(),
            &self.sender,
            self.address().into(),
            self.query().request_code_validation_base_fee().await?,
        )
        .await?;

        let code_id = CodeId::generate(code);

        let builder =
            self.instance
                .requestCodeValidation(code_id.into_bytes().into(), deadline, v, r, s);
        let builder =
            builder.sidecar_7594(SidecarBuilder::<SimpleCoder>::from_slice(code).build_7594()?);

        let receipt = builder
            .send()
            .await?
            .try_get_receipt_check_reverted()
            .await?;

        Ok((receipt, code_id))
    }

    /// Subscribes to `CodeGotValidated` events and blocks until the given `code_id` is confirmed, returning the result.
    pub async fn wait_for_code_validation(&self, code_id: CodeId) -> Result<CodeValidationResult> {
        let router_query = self.query();
        let mut stream = router_query
            .events()
            .code_got_validated()
            .subscribe()
            .await?;

        while let Some(result) = stream.next().await {
            if let Ok((
                CodeGotValidatedEvent {
                    code_id: event_code_id,
                    valid,
                },
                log,
            )) = result
                && event_code_id == code_id
            {
                return Ok(CodeValidationResult {
                    valid,
                    tx_hash: log.transaction_hash.map(|tx_hash| (*tx_hash).into()),
                    block_hash: log.block_hash.map(|block_hash| (*block_hash).into()),
                    block_number: log.block_number,
                });
            }
        }

        Err(anyhow!("Failed to define if code is validated"))
    }

    /// Creates a new program from the given `code_id` and `salt`, returning the transaction hash and the assigned `ActorId`.
    pub async fn create_program(
        &self,
        code_id: CodeId,
        salt: H256,
        override_initializer: Option<ActorId>,
    ) -> Result<(H256, ActorId)> {
        self.create_program_with_receipt(code_id, salt, override_initializer)
            .await
            .map(|(receipt, actor_id)| ((*receipt.transaction_hash).into(), actor_id))
    }

    /// Creates a new program and returns the full transaction receipt together with the assigned `ActorId`.
    pub async fn create_program_with_receipt(
        &self,
        code_id: CodeId,
        salt: H256,
        override_initializer: Option<ActorId>,
    ) -> Result<(TransactionReceipt, ActorId)> {
        let builder = self.instance.createProgram(
            code_id.into_bytes().into(),
            salt.to_fixed_bytes().into(),
            override_initializer
                .map(|initializer| {
                    let initializer = Address::try_from(initializer).expect("infallible");
                    AlloyAddress::new(initializer.0)
                })
                .unwrap_or_default(),
        );
        let receipt = builder
            .send()
            .await?
            .try_get_receipt_check_reverted()
            .await?;

        let mut actor_id = None;

        for log in receipt.inner.logs() {
            if log.topic0().cloned() == Some(signatures::PROGRAM_CREATED) {
                let event = crate::decode_log::<IRouter::ProgramCreated>(log)?;
                actor_id = Some((*event.actorId.into_word()).into());
                break;
            }
        }

        let actor_id = actor_id.ok_or_else(|| anyhow!("Couldn't find `ProgramCreated` log"))?;

        Ok((receipt, actor_id))
    }

    /// Creates a new program and seeds it with an initial executable WVara balance, returning the transaction hash and `ActorId`.
    pub async fn create_program_with_executable_balance(
        &self,
        code_id: CodeId,
        salt: H256,
        override_initializer: Option<ActorId>,
        initial_executable_balance: u128,
    ) -> Result<(H256, ActorId)> {
        self.create_program_with_executable_balance_and_receipt(
            code_id,
            salt,
            override_initializer,
            initial_executable_balance,
        )
        .await
        .map(|(receipt, actor_id)| ((*receipt.transaction_hash).into(), actor_id))
    }

    /// Creates a new program with an initial executable WVara balance and returns the full receipt together with the `ActorId`.
    ///
    /// Uses an EIP-712 permit to authorize the token transfer without a separate approval transaction.
    pub async fn create_program_with_executable_balance_and_receipt(
        &self,
        code_id: CodeId,
        salt: H256,
        override_initializer: Option<ActorId>,
        initial_executable_balance: u128,
    ) -> Result<(TransactionReceipt, ActorId)> {
        let Eip712PermitData { deadline, v, r, s } = Ethereum::prepare_permit_data(
            self.instance.provider(),
            self.wvara().query(),
            &self.sender,
            self.address().into(),
            initial_executable_balance,
        )
        .await?;

        let builder = self.instance.createProgramWithExecutableBalance(
            code_id.into_bytes().into(),
            salt.to_fixed_bytes().into(),
            override_initializer
                .map(|initializer| {
                    let initializer = Address::try_from(initializer).expect("infallible");
                    AlloyAddress::new(initializer.0)
                })
                .unwrap_or_default(),
            initial_executable_balance,
            deadline,
            v,
            r,
            s,
        );
        let receipt = builder
            .send()
            .await?
            .try_get_receipt_check_reverted()
            .await?;

        let mut actor_id = None;

        for log in receipt.inner.logs() {
            if log.topic0().cloned() == Some(signatures::PROGRAM_CREATED) {
                let event = crate::decode_log::<IRouter::ProgramCreated>(log)?;
                actor_id = Some((*event.actorId.into_word()).into());
                break;
            }
        }

        let actor_id = actor_id.ok_or_else(|| anyhow!("Couldn't find `ProgramCreated` log"))?;

        Ok((receipt, actor_id))
    }

    /// Creates a new program and registers an ABI interface actor for it, returning the transaction hash and the assigned `ActorId`.
    pub async fn create_program_with_abi_interface(
        &self,
        code_id: CodeId,
        salt: H256,
        override_initializer: Option<ActorId>,
        abi_interface: ActorId,
    ) -> Result<(H256, ActorId)> {
        self.create_program_with_abi_interface_with_receipt(
            code_id,
            salt,
            override_initializer,
            abi_interface,
        )
        .await
        .map(|(receipt, actor_id)| ((*receipt.transaction_hash).into(), actor_id))
    }

    /// Creates a new program with an ABI interface actor and returns the full receipt together with the assigned `ActorId`.
    pub async fn create_program_with_abi_interface_with_receipt(
        &self,
        code_id: CodeId,
        salt: H256,
        override_initializer: Option<ActorId>,
        abi_interface: ActorId,
    ) -> Result<(TransactionReceipt, ActorId)> {
        let abi_interface = Address::try_from(abi_interface).expect("infallible");
        let abi_interface = AlloyAddress::new(abi_interface.0);

        let builder = self.instance.createProgramWithAbiInterface(
            code_id.into_bytes().into(),
            salt.to_fixed_bytes().into(),
            override_initializer
                .map(|initializer| {
                    let initializer = Address::try_from(initializer).expect("infallible");
                    AlloyAddress::new(initializer.0)
                })
                .unwrap_or_default(),
            abi_interface,
        );
        let receipt = builder
            .send()
            .await?
            .try_get_receipt_check_reverted()
            .await?;
        let mut actor_id = None;

        for log in receipt.inner.logs() {
            if log.topic0().cloned() == Some(signatures::PROGRAM_CREATED) {
                let event = crate::decode_log::<IRouter::ProgramCreated>(log)?;
                actor_id = Some((*event.actorId.into_word()).into());
                break;
            }
        }

        let actor_id = actor_id.ok_or_else(|| anyhow!("Couldn't find `ProgramCreated` log"))?;

        Ok((receipt, actor_id))
    }

    /// Creates a new program with both an ABI interface and an initial executable WVara balance, returning the transaction hash and `ActorId`.
    pub async fn create_program_with_abi_interface_and_executable_balance(
        &self,
        code_id: CodeId,
        salt: H256,
        override_initializer: Option<ActorId>,
        abi_interface: ActorId,
        initial_executable_balance: u128,
    ) -> Result<(H256, ActorId)> {
        self.create_program_with_abi_interface_and_executable_balance_with_receipt(
            code_id,
            salt,
            override_initializer,
            abi_interface,
            initial_executable_balance,
        )
        .await
        .map(|(receipt, actor_id)| ((*receipt.transaction_hash).into(), actor_id))
    }

    /// Creates a new program with an ABI interface and an initial executable WVara balance, returning the full receipt and `ActorId`.
    ///
    /// Uses an EIP-712 permit to authorize the token transfer without a separate approval transaction.
    pub async fn create_program_with_abi_interface_and_executable_balance_with_receipt(
        &self,
        code_id: CodeId,
        salt: H256,
        override_initializer: Option<ActorId>,
        abi_interface: ActorId,
        initial_executable_balance: u128,
    ) -> Result<(TransactionReceipt, ActorId)> {
        let Eip712PermitData { deadline, v, r, s } = Ethereum::prepare_permit_data(
            self.instance.provider(),
            self.wvara().query(),
            &self.sender,
            self.address().into(),
            initial_executable_balance,
        )
        .await?;

        let abi_interface = Address::try_from(abi_interface).expect("infallible");
        let abi_interface = AlloyAddress::new(abi_interface.0);

        let builder = self
            .instance
            .createProgramWithAbiInterfaceAndExecutableBalance(
                code_id.into_bytes().into(),
                salt.to_fixed_bytes().into(),
                override_initializer
                    .map(|initializer| {
                        let initializer = Address::try_from(initializer).expect("infallible");
                        AlloyAddress::new(initializer.0)
                    })
                    .unwrap_or_default(),
                abi_interface,
                initial_executable_balance,
                deadline,
                v,
                r,
                s,
            );
        let receipt = builder
            .send()
            .await?
            .try_get_receipt_check_reverted()
            .await?;
        let mut actor_id = None;

        for log in receipt.inner.logs() {
            if log.topic0().cloned() == Some(signatures::PROGRAM_CREATED) {
                let event = crate::decode_log::<IRouter::ProgramCreated>(log)?;
                actor_id = Some((*event.actorId.into_word()).into());
                break;
            }
        }

        let actor_id = actor_id.ok_or_else(|| anyhow!("Couldn't find `ProgramCreated` log"))?;

        Ok((receipt, actor_id))
    }

    /// Submits a batch commitment with ECDSA signatures and waits for the receipt, returning the transaction hash.
    pub async fn commit_batch(
        &self,
        commitment: BatchCommitment,
        signatures: Vec<ContractSignature>,
    ) -> Result<H256> {
        self.commit_batch_pending(commitment, signatures)
            .await?
            .try_get_receipt_check_reverted()
            .await
            .map(|receipt| H256(receipt.transaction_hash.0))
    }

    /// Builds and broadcasts a batch commitment transaction, returning a [`PendingTransactionBuilder`] for further awaiting.
    ///
    /// Estimates gas with a state override that sets `reserved = 1` on the Router storage so the simulation can bypass the `Gear.blockIsPredecessor()` check,
    /// then applies the configured EIP-1559 fee cap and a hard gas-limit cap before sending.
    pub async fn commit_batch_pending(
        &self,
        commitment: BatchCommitment,
        signatures: Vec<ContractSignature>,
    ) -> Result<PendingTransactionBuilder<AlloyEthereum>> {
        let builder = self.instance.commitBatch(
            commitment.clone().into(),
            SignatureType::ECDSA as u8,
            signatures
                .into_iter()
                .map(|signature| Bytes::from(signature.into_pre_eip155_bytes()))
                .collect(),
        );

        let mut state_diff = HashMap::default();
        state_diff.insert(
            // keccak256(abi.encode(uint256(keccak256(bytes("router.storage.RouterV1"))) - 1)) & ~bytes32(uint256(0xff))
            fixed_bytes!("e3d827fd4fed52666d49a0df00f9cc2ac79f0f2378fc627e62463164801b6500"),
            // router.reserved = 1
            fixed_bytes!("0000000000000000000000000000000000000000000000000000000000000001"),
        );

        let mut state = HashMap::default();
        state.insert(
            *self.instance.address(),
            AccountOverride {
                state_diff: Some(state_diff),
                ..Default::default()
            },
        );

        let estimate_gas_builder = builder.clone().state(state);
        let calldata = estimate_gas_builder.calldata();
        let estimated_gas_limit = match estimate_gas_builder.estimate_gas().await {
            Ok(gas_limit) => gas_limit,
            Err(err) => {
                let latest_block =
                    Ethereum::get_latest_block_inner(self.instance.provider()).await?;
                let error = if let Some(router_error) =
                    err.as_decoded_interface_error::<IRouter::IRouterErrors>()
                {
                    format!("{router_error:?}")
                } else if let Some(gear_error) =
                    err.as_decoded_interface_error::<GearLib::GearErrors>()
                {
                    format!("{gear_error:?}")
                } else if let Some(bytes_error) = err.as_revert_data() {
                    format!("0x{}", hex::encode(bytes_error))
                } else {
                    format!("{err}")
                };
                log::error!(
                    "Failed to estimate gas for batch commitment: (error: {error}, block info: {latest_block}, calldata: 0x{}, batch commitment: {commitment:?})",
                    hex::encode(calldata)
                );
                return Err(anyhow!(
                    "Failed to estimate gas for batch commitment: {error}"
                ));
            }
        };

        let Eip1559Estimation {
            max_fee_per_gas, ..
        } = self
            .instance
            .provider()
            .estimate_eip1559_fees_with(self.eip1559_estimator.clone())
            .await?;

        let eip1559_max_fee_per_gas_in_wei = self
            .eip1559_max_fee_per_gas_in_gwei
            .saturating_mul(GWEI_TO_WEI as _);

        if eip1559_max_fee_per_gas_in_wei > 0 && max_fee_per_gas >= eip1559_max_fee_per_gas_in_wei {
            log::error!(
                "Estimated max fee per gas {max_fee_per_gas} wei is higher than the configured maximum of {eip1559_max_fee_per_gas_in_wei} wei, refusing to commit batch (commitment: {commitment:?})"
            );
            return Err(anyhow!(
                "Estimated max fee per gas {max_fee_per_gas} wei is higher than the configured maximum of {eip1559_max_fee_per_gas_in_wei} wei, refusing to commit batch",
            ));
        }

        let gas_limit = estimated_gas_limit + Self::GEAR_BLOCK_IS_PREDECESSOR_GAS;

        if gas_limit > Self::TX_GAS_LIMIT_CAP {
            log::error!(
                "Estimated gas limit {gas_limit} is too high for batch commitment: {commitment:?}",
            );
            return Err(anyhow!(
                "Estimated gas limit {gas_limit} is too high for batch commitment",
            ));
        }

        builder.gas(gas_limit).send().await.map_err(Into::into)
    }
}

/// Outcome of a code validation event observed from the Router contract.
#[derive(Clone, Debug, Serialize)]
pub struct CodeValidationResult {
    /// Whether the submitted code was accepted as valid by the validators.
    pub valid: bool,
    /// Hash of the transaction that carried the `CodeGotValidated` event, if available.
    pub tx_hash: Option<H256>,
    /// Hash of the block containing the validation event, if available.
    pub block_hash: Option<H256>,
    /// Number of the block containing the validation event, if available.
    pub block_number: Option<u64>,
}

/// Read-only handle for querying on-chain state from the `IRouter` contract.
///
/// Uses an unauthenticated `RootProvider` so no private key is required.
/// Obtain an instance via [`Router::query`] or [`RouterQuery::new`].
#[derive(Clone)]
pub struct RouterQuery {
    instance: QueryInstance,
}

impl RouterQuery {
    /// Connects to the given JSON-RPC endpoint and constructs a `RouterQuery` for the specified contract address.
    pub async fn new(rpc_url: &str, router_address: impl Into<AlloyAddress>) -> Result<Self> {
        let provider = ProviderBuilder::default().connect(rpc_url).await?;

        Ok(Self {
            instance: QueryInstance::new(router_address.into(), provider),
        })
    }

    /// Constructs a `RouterQuery` from an already-connected provider without performing a network call.
    pub fn from_provider(router_address: impl Into<AlloyAddress>, provider: RootProvider) -> Self {
        Self {
            instance: QueryInstance::new(router_address.into(), provider),
        }
    }

    /// Returns a [`RouterEvents`] helper that exposes typed event subscription builders for this query.
    pub fn events(&self) -> RouterEvents<'_> {
        RouterEvents { query: self }
    }

    // TODO: move StorageView into ethexe-common and export

    /// Fetches the full contract storage snapshot at the latest block.
    pub async fn storage_view(&self) -> Result<IRouter::StorageView> {
        self.storage_view_at(BlockId::latest()).await
    }

    /// Fetches the full contract storage snapshot at the given block.
    pub async fn storage_view_at(&self, id: impl IntoBlockId) -> Result<IRouter::StorageView> {
        self.instance
            .storageView()
            .call()
            .block(id.into_block_id())
            .await
            .map_err(Into::into)
    }

    /// Returns the hash of the Ethereum block used as the genesis anchor for this Router deployment.
    pub async fn genesis_block_hash(&self) -> Result<H256> {
        self.instance
            .genesisBlockHash()
            .call()
            .await
            .map(|res| H256(*res))
            .map_err(Into::into)
    }

    /// Returns the timestamp of the genesis block used as the Router's starting point.
    pub async fn genesis_timestamp(&self) -> Result<u64> {
        self.instance
            .genesisTimestamp()
            .call()
            .await
            .map(uint48_to_u64)
            .map_err(Into::into)
    }

    /// Returns the digest of the most recently committed batch.
    pub async fn latest_committed_batch_hash(&self) -> Result<Digest> {
        self.instance
            .latestCommittedBatchHash()
            .call()
            .await
            .map(|res| Digest(res.0))
            .map_err(Into::into)
    }

    /// Returns the timestamp of the most recently committed batch.
    pub async fn latest_committed_batch_timestamp(&self) -> Result<u64> {
        self.instance
            .latestCommittedBatchTimestamp()
            .call()
            .await
            .map(uint48_to_u64)
            .map_err(Into::into)
    }

    /// Returns the address of the Mirror implementation contract used by the Router for program proxies.
    pub async fn mirror_impl(&self) -> Result<Address> {
        self.instance
            .mirrorImpl()
            .call()
            .await
            .map(|res| res.into())
            .map_err(Into::into)
    }

    /// Returns the address of the Wrapped Vara (WVara) ERC-20 token contract linked to this Router.
    pub async fn wvara_address(&self) -> Result<Address> {
        self.instance
            .wrappedVara()
            .call()
            .await
            .map(|res| res.into())
            .map_err(Into::into)
    }

    /// Returns the address of the Middleware contract responsible for validator election and permissions.
    pub async fn middleware_address(&self) -> Result<Address> {
        self.instance
            .middleware()
            .call()
            .await
            .map(|res| res.into())
            .map_err(Into::into)
    }

    /// Returns the aggregated FROST public key of the current validator set used for signature verification.
    pub async fn validators_aggregated_public_key(&self) -> Result<AggregatedPublicKey> {
        self.instance
            .validatorsAggregatedPublicKey()
            .call()
            .await
            .map(|res| AggregatedPublicKey {
                x: uint256_to_u256(res.x),
                y: uint256_to_u256(res.y),
            })
            .map_err(Into::into)
    }

    /// Returns the raw bytes of the verifiable secret sharing commitment for the current validator set.
    pub async fn validators_verifiable_secret_sharing_commitment(&self) -> Result<Vec<u8>> {
        self.instance
            .validatorsVerifiableSecretSharingCommitment()
            .call()
            .await
            .map(|res| res.into())
            .map_err(Into::into)
    }

    /// Returns `true` if every address in `validators` is a member of the current validator set.
    pub async fn are_validators(
        &self,
        validators: impl IntoIterator<Item = Address>,
    ) -> Result<bool> {
        let addresses: Vec<AlloyAddress> = validators.into_iter().map(|addr| addr.into()).collect();
        self.instance
            .areValidators(addresses)
            .call()
            .await
            .map_err(Into::into)
    }

    /// Returns `true` if the given address is a member of the current validator set.
    pub async fn is_validator(&self, validator: Address) -> Result<bool> {
        let address: AlloyAddress = validator.into();
        self.instance
            .isValidator(address)
            .call()
            .await
            .map_err(Into::into)
    }

    /// Returns the signing threshold as a `(numerator, denominator)` fraction of the validator count required to commit a batch.
    pub async fn signing_threshold_fraction(&self) -> Result<(u128, u128)> {
        self.instance
            .signingThresholdFraction()
            .call()
            .await
            .map(|res| (res.thresholdNumerator, res.thresholdDenominator))
            .map_err(Into::into)
    }

    /// Returns the list of current validators at the latest block.
    pub async fn validators(&self) -> Result<ValidatorsVec> {
        self.validators_at(BlockId::latest()).await
    }

    /// Returns the list of validators at the specified block.
    pub async fn validators_at(&self, id: impl IntoBlockId) -> Result<ValidatorsVec> {
        let validators: Vec<_> = self
            .instance
            .validators()
            .call()
            .block(id.into_block_id())
            .await
            .map(|res| res.into_iter().map(|v| Address(v.into())).collect())
            .map_err(Into::<anyhow::Error>::into)?;
        validators.try_into().map_err(Into::into)
    }

    /// Returns the number of validators in the current set.
    pub async fn validators_count(&self) -> Result<u64> {
        self.instance
            .validatorsCount()
            .call()
            .await
            .map(|res| res.to())
            .map_err(Into::into)
    }

    /// Returns the minimum number of validator signatures required to commit a batch.
    pub async fn validators_threshold(&self) -> Result<u64> {
        self.instance
            .validatorsThreshold()
            .call()
            .await
            .map(|res| res.to())
            .map_err(Into::into)
    }

    /// Returns the on-chain computation settings that govern program execution parameters.
    pub async fn compute_settings(&self) -> Result<ComputationSettings> {
        self.instance
            .computeSettings()
            .call()
            .await
            .map(|res| res.into())
            .map_err(Into::into)
    }

    /// Returns the validation state of a single code blob identified by `code_id`.
    pub async fn code_state(&self, code_id: CodeId) -> Result<CodeState> {
        self.instance
            .codeState(code_id.into_bytes().into())
            .call()
            .await
            .map(CodeState::from)
            .map_err(Into::into)
    }

    /// Returns the validation states for multiple code blobs at the latest block.
    pub async fn codes_states(
        &self,
        code_ids: impl IntoIterator<Item = CodeId>,
    ) -> Result<Vec<CodeState>> {
        self.instance
            .codesStates(
                code_ids
                    .into_iter()
                    .map(|c| c.into_bytes().into())
                    .collect(),
            )
            .call()
            .await
            .map(|res| res.into_iter().map(CodeState::from).collect())
            .map_err(Into::into)
    }

    /// Returns the validation states for multiple code blobs at the specified block.
    pub async fn codes_states_at(
        &self,
        code_ids: impl IntoIterator<Item = CodeId>,
        id: impl IntoBlockId,
    ) -> Result<Vec<CodeState>> {
        self.instance
            .codesStates(
                code_ids
                    .into_iter()
                    .map(|c| c.into_bytes().into())
                    .collect(),
            )
            .call()
            .block(id.into_block_id())
            .await
            .map(|res| res.into_iter().map(CodeState::from).collect())
            .map_err(Into::into)
    }

    /// Returns the `CodeId` of the code backing the given program, or `None` if the program does not exist.
    pub async fn program_code_id(&self, program_id: ActorId) -> Result<Option<CodeId>> {
        let program_id = Address::try_from(program_id).expect("infallible");
        let program_id = AlloyAddress::new(program_id.0);
        let code_id = self.instance.programCodeId(program_id).call().await?;
        let code_id = Some(CodeId::new(code_id.0)).filter(|&code_id| code_id != CodeId::zero());
        Ok(code_id)
    }

    /// Returns the `CodeId` for each program in the given list at the latest block.
    pub async fn programs_code_ids(
        &self,
        program_ids: impl IntoIterator<Item = ActorId>,
    ) -> Result<Vec<CodeId>> {
        self.instance
            .programsCodeIds(
                program_ids
                    .into_iter()
                    .map(|p| {
                        let program_id = Address::try_from(p).expect("infallible");
                        AlloyAddress::new(program_id.0)
                    })
                    .collect(),
            )
            .call()
            .await
            .map(|res| res.into_iter().map(|c| CodeId::new(c.0)).collect())
            .map_err(Into::into)
    }

    /// Returns the `CodeId` for each program in the given list at the specified block.
    pub async fn programs_code_ids_at(
        &self,
        program_ids: impl IntoIterator<Item = ActorId>,
        id: impl IntoBlockId,
    ) -> Result<Vec<CodeId>> {
        self.instance
            .programsCodeIds(
                program_ids
                    .into_iter()
                    .map(|p| {
                        let program_id = Address::try_from(p).expect("infallible");
                        AlloyAddress::new(program_id.0)
                    })
                    .collect(),
            )
            .call()
            .block(id.into_block_id())
            .await
            .map(|res| res.into_iter().map(|c| CodeId::new(c.0)).collect())
            .map_err(Into::into)
    }

    /// Returns the total number of programs registered in the Router at the latest block.
    pub async fn programs_count(&self) -> Result<u64> {
        self.programs_count_at(BlockId::latest()).await
    }

    /// Returns the total number of programs registered in the Router at the specified block.
    pub async fn programs_count_at(&self, id: impl IntoBlockId) -> Result<u64> {
        let count = self
            .instance
            .programsCount()
            .call()
            .block(id.into_block_id())
            .await?;
        // it's impossible to ever reach 18 quintillion programs (maximum of u64)
        let count: u64 = count.try_into().expect("infallible");
        Ok(count)
    }

    /// Returns the total number of validated code blobs at the latest block.
    pub async fn validated_codes_count(&self) -> Result<u64> {
        self.validated_codes_count_at(BlockId::latest()).await
    }

    /// Returns the total number of validated code blobs at the specified block.
    pub async fn validated_codes_count_at(&self, id: impl IntoBlockId) -> Result<u64> {
        let count = self
            .instance
            .validatedCodesCount()
            .call()
            .block(id.into_block_id())
            .await?;
        // it's impossible to ever reach 18 quintillion programs (maximum of u64)
        let count: u64 = count.try_into().expect("infallible");
        Ok(count)
    }

    /// Returns the base WVara fee charged per code validation request.
    pub async fn request_code_validation_base_fee(&self) -> Result<u128> {
        let base_fee = self.instance.requestCodeValidationBaseFee().call().await?;
        Ok(base_fee.try_into().expect("infallible"))
    }

    /// Returns the extra WVara fee that may be charged on top of the base fee for code validation.
    pub async fn request_code_validation_extra_fee(&self) -> Result<u128> {
        let extra_fee = self.instance.requestCodeValidationExtraFee().call().await?;
        Ok(extra_fee.try_into().expect("infallible"))
    }

    /// Returns the Router's configured timeline parameters governing era and batch scheduling.
    pub async fn timelines(&self) -> Result<Timelines> {
        self.instance
            .timelines()
            .call()
            .await
            .map(|res| res.into())
            .map_err(Into::into)
    }
}

/// Factory for typed event subscription builders scoped to a single [`RouterQuery`].
pub struct RouterEvents<'a> {
    query: &'a RouterQuery,
}

impl<'a> RouterEvents<'a> {
    /// Returns a builder that subscribes to all Router contract events.
    pub fn all(&self) -> AllEventsBuilder<'a> {
        AllEventsBuilder::new(self.query)
    }

    /// Returns a builder for subscribing to `BatchCommitted` events.
    pub fn batch_committed(&self) -> BatchCommittedEventBuilder<'a> {
        BatchCommittedEventBuilder::new(self.query)
    }

    /// Returns a builder for subscribing to `MBCommitted` (announces-chain head committed) events.
    pub fn mb_committed(&self) -> MBCommittedEventBuilder<'a> {
        MBCommittedEventBuilder::new(self.query)
    }

    /// Returns a builder for subscribing to `EBCommitted` (Ethereum-block committed) events.
    pub fn eb_committed(&self) -> EBCommittedEventBuilder<'a> {
        EBCommittedEventBuilder::new(self.query)
    }

    /// Returns a builder for subscribing to `CodeGotValidated` events.
    pub fn code_got_validated(&self) -> CodeGotValidatedEventBuilder<'a> {
        CodeGotValidatedEventBuilder::new(self.query)
    }

    /// Returns a builder for subscribing to `CodeValidationRequested` events.
    pub fn code_validation_requested(&self) -> CodeValidationRequestedEventBuilder<'a> {
        CodeValidationRequestedEventBuilder::new(self.query)
    }

    /// Returns a builder for subscribing to `ValidatorsCommittedForEra` events.
    pub fn validators_committed_for_era(&self) -> ValidatorsCommittedForEraEventBuilder<'a> {
        ValidatorsCommittedForEraEventBuilder::new(self.query)
    }

    /// Returns a builder for subscribing to `ComputationSettingsChanged` events.
    pub fn computation_settings_changed(&self) -> ComputationSettingsChangedEventBuilder<'a> {
        ComputationSettingsChangedEventBuilder::new(self.query)
    }

    /// Returns a builder for subscribing to `ProgramCreated` events.
    pub fn program_created(&self) -> ProgramCreatedEventBuilder<'a> {
        ProgramCreatedEventBuilder::new(self.query)
    }

    /// Returns a builder for subscribing to `StorageSlotChanged` events.
    pub fn storage_slot_changed(&self) -> StorageSlotChangedEventBuilder<'a> {
        StorageSlotChangedEventBuilder::new(self.query)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::deploy::EthereumDeployer;
    use alloy::{eips::BlockId, node_bindings::Anvil};
    use gsigner::Signer;

    #[tokio::test]
    async fn inexistent_code_is_unknown() {
        let anvil = Anvil::new().spawn();

        let signer = Signer::memory();
        let alice = signer
            .import(
                "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80"
                    .parse()
                    .unwrap(),
            )
            .unwrap();

        let states =
            EthereumDeployer::new(anvil.endpoint_url().as_str(), signer, alice.to_address())
                .await
                .unwrap()
                .deploy()
                .await
                .unwrap()
                .router()
                .query()
                .codes_states_at([CodeId::new([0xfe; 32])], BlockId::latest())
                .await
                .unwrap();
        assert_eq!(states, vec![CodeState::Unknown]);
    }

    #[tokio::test]
    async fn storage_view() {
        let anvil = Anvil::new().spawn();

        let signer = Signer::memory();
        let alice = signer
            .import(
                "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80"
                    .parse()
                    .unwrap(),
            )
            .unwrap();

        let storage =
            EthereumDeployer::new(anvil.endpoint_url().as_str(), signer, alice.to_address())
                .await
                .unwrap()
                .deploy()
                .await
                .unwrap()
                .router()
                .query()
                .storage_view_at(BlockId::latest())
                .await
                .unwrap();
        assert!(storage.validationSettings.validators0.useFromTimestamp > 0);
        assert_eq!(storage.validationSettings.validators0.list.len(), 1);
        assert_eq!(storage.validationSettings.validators1.useFromTimestamp, 0);
    }
}
