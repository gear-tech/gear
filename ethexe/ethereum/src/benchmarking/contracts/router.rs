// This file is part of Gear.
//
// Copyright (C) 2024-2026 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use crate::{
    abi::{Gear, IRouter, ITransparentUpgradeableProxy},
    benchmarking::{
        SimulationContext,
        contracts::{MirrorImpl, RouterImpl, WrappedVara},
    },
};
use alloy::sol_types::{SolCall, SolConstructor};
use anyhow::{Result, anyhow, bail};
use ethexe_common::{
    Digest, ToDigest,
    gear::{BatchCommitment, ChainCommitment, CodeCommitment, SignatureType},
};
use gear_core::ids::prelude::CodeIdExt as _;
use gprimitives::{ActorId, CodeId, H256};
use gsigner::secp256k1::Secp256k1SignerExt;
use revm::{
    ExecuteCommitEvm, ExecuteEvm, InspectEvm,
    bytecode::Bytecode,
    context::{ContextTr, JournalTr, TxEnv},
    context_interface::result::{ExecutionResult, Output},
    primitives::{Address, B256, Bytes, U256, eip4844::VERSIONED_HASH_VERSION_KZG},
};

#[derive(Debug, Clone, Copy)]
pub enum ContractImplKind {
    Regular,
    WithInstrumentation,
}

#[derive(Debug)]
pub struct GasResult {
    pub execution_gas: u64,
    pub calldata_gas: u64,
}

impl GasResult {
    pub fn total_gas(&self) -> u64 {
        self.execution_gas
            .checked_add(self.calldata_gas)
            .expect("infallible")
    }

    pub fn total_tx_gas(&self) -> u64 {
        const BASE_FEE: u64 = 21_000;
        self.total_gas().checked_add(BASE_FEE).expect("infallible")
    }
}

#[derive(Debug)]
pub enum ExecutionMode {
    Execute,
    ExecuteAndCommit,
    ExecuteAndInspect,
}

pub struct Router<'a> {
    context: &'a mut SimulationContext,
    router_impl: RouterImpl,
    proxy_address: Address,
    mirror_impl: MirrorImpl,
}

impl<'a> Router<'a> {
    pub fn deploy(
        context: &'a mut SimulationContext,
        precomputed_mirror_impl: Address,
        wrapped_vara: &WrappedVara,
    ) -> Result<Self> {
        let router_impl = RouterImpl::deploy(context)?;

        context.next_block();

        let middleware_address = Address::ZERO;
        let aggregated_public_key = Gear::AggregatedPublicKey {
            x: "0x1b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f".parse()?,
            y: "0x70beaf8f588b541507fed6a642c5ab42dfdf8120a7f639de5122d47a69a8e8d1".parse()?,
        };

        let router_proxy = Self::deploy_proxy(
            context,
            router_impl.address(),
            precomputed_mirror_impl,
            wrapped_vara,
            middleware_address,
            aggregated_public_key,
            context.validators(),
        )?;

        let journal = context.evm().journal_mut();
        journal.balance_incr(router_proxy, u64::MAX.try_into().expect("infallible"))?;
        let state = journal.finalize();
        context.evm().commit(state);

        let mirror_impl = MirrorImpl::deploy(context, router_proxy)?;
        assert_eq!(mirror_impl.address(), precomputed_mirror_impl);

        Ok(Self {
            context,
            router_impl,
            proxy_address: router_proxy,
            mirror_impl,
        })
    }

    fn deploy_proxy(
        context: &mut SimulationContext,
        router_impl: Address,
        mirror_impl: Address,
        wrapped_vara: &WrappedVara,
        middleware_address: Address,
        aggregated_public_key: Gear::AggregatedPublicKey,
        validators: Vec<Address>,
    ) -> Result<Address> {
        let deployer_address = context.deployer_address();
        let deployer_nonce = context.deployer_nonce();

        let ExecutionResult::Success {
            output: Output::Create(_, Some(router_proxy)),
            ..
        } = context.evm().transact_commit(
            TxEnv::builder()
                .caller(deployer_address)
                .create()
                .data(
                    [
                        &ITransparentUpgradeableProxy::BYTECODE[..],
                        &SolConstructor::abi_encode(
                            &ITransparentUpgradeableProxy::constructorCall {
                                _logic: router_impl,
                                initialOwner: deployer_address,
                                _data: Bytes::copy_from_slice(
                                    &IRouter::initializeCall {
                                        _owner: deployer_address,
                                        _mirror: mirror_impl,
                                        _wrappedVara: wrapped_vara.proxy_address(),
                                        _middleware: middleware_address,
                                        _eraDuration: U256::from(24 * 60 * 60),
                                        _electionDuration: U256::from(2 * 60 * 60),
                                        _validationDelay: U256::from(5 * 60),
                                        _aggregatedPublicKey: aggregated_public_key,
                                        _verifiableSecretSharingCommitment: Bytes::new(),
                                        _validators: validators,
                                    }
                                    .abi_encode(),
                                ),
                            },
                        )[..],
                    ]
                    .concat()
                    .into(),
                )
                .nonce(deployer_nonce)
                .build()
                .map_err(|_| anyhow!("failed to build TxEnv"))?,
        )?
        else {
            bail!("failed to deploy TransparentUpgradeableProxy contract (Router proxy)");
        };

        context.increment_deployer_nonce();

        Ok(router_proxy)
    }

    pub fn context(&mut self) -> &mut SimulationContext {
        self.context
    }

    pub fn router_impl(&self) -> &RouterImpl {
        &self.router_impl
    }

    pub fn proxy_address(&self) -> Address {
        self.proxy_address
    }

    fn mirror_impl(&self) -> &MirrorImpl {
        &self.mirror_impl
    }

    fn switch_to_mirror_impl(&mut self, kind: ContractImplKind) -> Result<()> {
        let mirror_impl = self.mirror_impl();
        let address = mirror_impl.address();
        let code = Bytecode::new_legacy(
            match kind {
                ContractImplKind::Regular => mirror_impl.mirror_impl_bytecode(),
                ContractImplKind::WithInstrumentation => {
                    mirror_impl.mirror_impl_with_instrumentation_bytecode()
                }
            }
            .clone(),
        );

        let journal = self.context.evm().journal_mut();
        journal.load_account(address)?;
        journal.set_code(address, code);
        let state = journal.finalize();
        self.context.evm().commit(state);

        Ok(())
    }

    fn switch_to_router_impl(&mut self, kind: ContractImplKind) -> Result<()> {
        let address = self.router_impl.address();
        let code = Bytecode::new_legacy(
            match kind {
                ContractImplKind::Regular => self.router_impl.router_impl_bytecode(),
                ContractImplKind::WithInstrumentation => {
                    self.router_impl.router_impl_with_instrumentation_bytecode()
                }
            }
            .clone(),
        );

        let journal = self.context.evm().journal_mut();
        journal.load_account(address)?;
        journal.set_code(address, code);
        let state = journal.finalize();
        self.context.evm().commit(state);

        Ok(())
    }

    pub fn switch_to_impl(&mut self, contract_impl_kind: ContractImplKind) -> Result<()> {
        self.switch_to_mirror_impl(contract_impl_kind)?;
        self.switch_to_router_impl(contract_impl_kind)?;
        Ok(())
    }

    fn latest_committed_batch_hash(&mut self) -> Result<Digest> {
        let deployer_address = self.context.deployer_address();
        let deployer_nonce = self.context.deployer_nonce();

        let proxy_address = self.proxy_address();

        let ExecutionResult::Success {
            output: Output::Call(hash),
            ..
        } = self
            .context
            .evm()
            .transact(
                TxEnv::builder()
                    .caller(deployer_address)
                    .call(proxy_address)
                    .data(IRouter::latestCommittedBatchHashCall {}.abi_encode().into())
                    .nonce(deployer_nonce)
                    .build()
                    .map_err(|_| anyhow!("failed to build TxEnv"))?,
            )?
            .result
        else {
            bail!("failed to get latest committed batch hash");
        };

        Ok(Digest(H256::from_slice(&hash).to_fixed_bytes()))
    }

    pub fn lookup_genesis_hash(&mut self) -> Result<()> {
        let deployer_address = self.context.deployer_address();
        let deployer_nonce = self.context.deployer_nonce();

        let proxy_address = self.proxy_address();

        self.context.next_block();

        let ExecutionResult::Success { .. } = self.context.evm().transact_commit(
            TxEnv::builder()
                .caller(deployer_address)
                .call(proxy_address)
                .data(IRouter::lookupGenesisHashCall {}.abi_encode().into())
                .nonce(deployer_nonce)
                .build()
                .map_err(|_| anyhow!("failed to build TxEnv"))?,
        )?
        else {
            bail!("failed to lookup genesis hash");
        };

        self.context.increment_deployer_nonce();

        Ok(())
    }

    pub fn request_code_validation(&mut self, code: &[u8]) -> Result<CodeId> {
        let deployer_address = self.context.deployer_address();
        let deployer_nonce = self.context.deployer_nonce();

        let proxy_address = self.proxy_address();
        let code_id = CodeId::generate(code);

        let ExecutionResult::Success { .. } = self.context.evm().transact_commit(
            TxEnv::builder()
                .caller(deployer_address)
                .call(proxy_address)
                .data(
                    IRouter::requestCodeValidationCall {
                        _codeId: code_id.into_bytes().into(),
                    }
                    .abi_encode()
                    .into(),
                )
                .nonce(deployer_nonce)
                .blob_hashes(vec![B256::from([VERSIONED_HASH_VERSION_KZG; 32])])
                .max_fee_per_blob_gas(1)
                .build()
                .map_err(|_| anyhow!("failed to build TxEnv"))?,
        )?
        else {
            bail!("failed to request code validation");
        };

        self.context.increment_deployer_nonce();

        Ok(code_id)
    }

    pub fn create_program(
        &mut self,
        code_id: CodeId,
        salt: H256,
        override_initializer: Option<ActorId>,
    ) -> Result<ActorId> {
        let deployer_address = self.context.deployer_address();
        let deployer_nonce = self.context.deployer_nonce();

        let proxy_address = self.proxy_address();

        let ExecutionResult::Success {
            output: Output::Call(actor_id),
            ..
        } = self.context.evm().transact_commit(
            TxEnv::builder()
                .caller(deployer_address)
                .call(proxy_address)
                .data(
                    IRouter::createProgramCall {
                        _codeId: code_id.into_bytes().into(),
                        _salt: salt.0.into(),
                        _overrideInitializer: override_initializer
                            .map(|initializer| initializer.to_address_lossy().0.into())
                            .unwrap_or_default(),
                    }
                    .abi_encode()
                    .into(),
                )
                .nonce(deployer_nonce)
                .build()
                .map_err(|_| anyhow!("failed to build TxEnv"))?,
        )?
        else {
            bail!("failed to create program");
        };

        self.context.increment_deployer_nonce();

        Ok(actor_id.as_ref().try_into().expect("infallible"))
    }

    fn commit_batch_tx(&mut self, batch: BatchCommitment) -> Result<(u64, TxEnv)> {
        let batch_digest = batch.to_digest();

        let signatures = self
            .context
            .validators_with_keys()
            .iter()
            .map(|(signer, pubkey, _)| {
                Bytes::from(
                    signer
                        .sign_for_contract_digest(
                            self.proxy_address().into(),
                            *pubkey,
                            batch_digest,
                            None,
                        )
                        .expect("infallible")
                        .into_pre_eip155_bytes(),
                )
            })
            .take(self.context.min_signers() as _)
            .collect::<Vec<_>>();

        let data: Bytes = IRouter::commitBatchCall {
            _batch: batch.into(),
            _signatureType: SignatureType::ECDSA as u8,
            _signatures: signatures,
        }
        .abi_encode()
        .into();

        let calldata = data.as_ref();

        let zero_bytes = calldata.iter().filter(|&&b| b == 0).count();
        let non_zero_bytes = calldata.len() - zero_bytes;

        let calldata_gas = ((16 * non_zero_bytes) + (4 * zero_bytes)) as u64;

        let tx = TxEnv::builder()
            .caller(self.context.deployer_address())
            .call(self.proxy_address())
            .data(data)
            .nonce(self.context.deployer_nonce())
            .build()
            .map_err(|_| anyhow!("failed to build TxEnv"))?;

        Ok((calldata_gas, tx))
    }

    fn commit_batch(
        &mut self,
        batch: BatchCommitment,
        execution_mode: ExecutionMode,
    ) -> Result<GasResult> {
        let (calldata_gas, tx) = self.commit_batch_tx(batch)?;

        let execution_result = match execution_mode {
            ExecutionMode::Execute => self.context.evm().transact(tx)?.result,
            ExecutionMode::ExecuteAndCommit => self.context.evm().transact_commit(tx)?,
            ExecutionMode::ExecuteAndInspect => self.context.evm().inspect_tx(tx)?.result,
        };
        let ExecutionResult::Success {
            gas_used: execution_gas,
            ..
        } = execution_result
        else {
            bail!("failed to commit batch");
        };

        if let ExecutionMode::ExecuteAndCommit = execution_mode {
            self.context.increment_deployer_nonce();
        }

        const BASE_FEE: u64 = 21_000;

        let execution_gas = execution_gas
            .checked_sub(BASE_FEE)
            .expect("infallible")
            .checked_sub(calldata_gas)
            .expect("infallible");

        Ok(GasResult {
            execution_gas,
            calldata_gas,
        })
    }

    pub fn commit_batch_simple(
        &mut self,
        chain_commitment: Option<ChainCommitment>,
        code_commitments: Vec<CodeCommitment>,
        execution_mode: ExecutionMode,
    ) -> Result<()> {
        self.context.next_block();

        let latest_committed_batch_hash = self.latest_committed_batch_hash()?;

        self.commit_batch(
            BatchCommitment {
                block_hash: self.context.parent_block_hash()?,
                timestamp: self.context.parent_block_timestamp_u64(),
                previous_batch: latest_committed_batch_hash,
                expiry: 1,
                chain_commitment,
                code_commitments,
                validators_commitment: None,
                rewards_commitment: None,
            },
            execution_mode,
        )?;

        Ok(())
    }

    pub fn estimate_commit_batch_gas_between_topics(
        &mut self,
        chain_commitment: Option<ChainCommitment>,
        code_commitments: Vec<CodeCommitment>,
        contract_address: Address,
        start_topic: u32,
        end_topic: u32,
    ) -> Result<GasResult> {
        self.context.evm().inspector.topic_bounded_gas_inspector(
            contract_address,
            U256::try_from(start_topic).expect("infallible").into(),
            U256::try_from(end_topic).expect("infallible").into(),
        );

        let GasResult { calldata_gas, .. } = self.estimate_commit_batch_gas(
            chain_commitment,
            code_commitments,
            ExecutionMode::ExecuteAndInspect,
        )?;
        let execution_gas = self.context.evm().inspector.gas_diff().expect("infallible");

        Ok(GasResult {
            execution_gas,
            calldata_gas,
        })
    }

    pub fn estimate_commit_batch_gas(
        &mut self,
        chain_commitment: Option<ChainCommitment>,
        code_commitments: Vec<CodeCommitment>,
        execution_mode: ExecutionMode,
    ) -> Result<GasResult> {
        let expiry = 3;

        for _ in 0..expiry {
            self.context.next_block();
        }

        let latest_committed_batch_hash = self.latest_committed_batch_hash()?;

        let gas_result = self.commit_batch(
            BatchCommitment {
                block_hash: self.context.block_hash(
                    self.context
                        .block_number()
                        .checked_sub(U256::from(3))
                        .expect("infallible"),
                )?,
                timestamp: self
                    .context
                    .block_timestamp_u64()
                    .checked_sub(3)
                    .expect("infallible"),
                previous_batch: latest_committed_batch_hash,
                expiry,
                chain_commitment,
                code_commitments,
                validators_commitment: None,
                rewards_commitment: None,
            },
            execution_mode,
        )?;

        for _ in 0..expiry {
            self.context.prev_block();
        }

        Ok(gas_result)
    }
}
