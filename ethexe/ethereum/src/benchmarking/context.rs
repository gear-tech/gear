// This file is part of Gear.
//
// Copyright (C) 2026 Gear Technologies Inc.
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
    abi::{Gear, utils},
    benchmarking::{
        contracts::{ContractImplKind, ExecutionMode, Router, WrappedVara},
        extensions::CalldataGasExt,
        inspector::SimulationInspector,
        mnemonic,
    },
};
use alloy::sol_types::SolValue;
use anyhow::Result;
use ethexe_common::{
    Announce, HashOf,
    gear::{ChainCommitment, CodeCommitment, Message, StateTransition, ValueClaim},
};
use gprimitives::{ActorId, CodeId, H256};
use gsigner::secp256k1::{PublicKey, Signer};
use revm::{
    DatabaseRef, ExecuteCommitEvm, MainBuilder, MainContext, MainnetEvm,
    context::{BlockEnv, CfgEnv, Context, ContextTr, JournalTr, TxEnv},
    database::CacheDB,
    database_interface::EmptyDB,
    primitives::{Address, Bytes, U256},
};

pub struct SimulationContext {
    evm: MainnetEvm<Context<BlockEnv, TxEnv, CfgEnv, CacheDB<EmptyDB>>, SimulationInspector>,
    block_number: U256,
    block_timestamp: U256,
    deployer_address: Address,
    deployer_nonce: u64,
    validators_with_keys: Vec<(Signer, PublicKey, Address)>,
}

impl SimulationContext {
    const VALIDATOR_COUNT: u32 = 4;
    const MIRROR_DEPLOYMENT_NONCE_OFFSET: u64 = 2;

    const PERFORM_STATE_TRANSITION_BEFORE_RETRIEVE_ETHER: u32 = 3;
    const PERFORM_STATE_TRANSITION_AFTER_RETRIEVE_ETHER: u32 = 4;

    pub fn new() -> Result<Self> {
        let block_number = U256::ZERO;
        let block_timestamp = U256::ZERO;

        let mut evm = Context::mainnet()
            .with_db(CacheDB::<EmptyDB>::default())
            .with_block(BlockEnv {
                number: block_number,
                timestamp: block_timestamp,
                ..Default::default()
            })
            .build_mainnet_with_inspector(SimulationInspector::default());

        let (_, _, deployer_address) = mnemonic::derive_signer(0)?;
        let deployer_nonce = 0;

        let journal = evm.journal_mut();
        journal.balance_incr(deployer_address, u64::MAX.try_into().expect("infallible"))?;
        let state = journal.finalize();
        evm.commit(state);

        let validators_with_keys = (1..=Self::VALIDATOR_COUNT)
            .map(mnemonic::derive_signer)
            .collect::<Result<Vec<_>>>()?;

        Ok(Self {
            evm,
            block_number,
            block_timestamp,
            deployer_address,
            deployer_nonce,
            validators_with_keys,
        })
    }

    pub fn initialize(&mut self) -> Result<InitializedSimulationContext<'_>> {
        let wrapped_vara = WrappedVara::deploy(self)?;

        let precomputed_mirror_impl = self.deployer_address.create(
            self.deployer_nonce
                .checked_add(Self::MIRROR_DEPLOYMENT_NONCE_OFFSET)
                .expect("infallible"),
        );

        let mut router = Router::deploy(self, precomputed_mirror_impl, &wrapped_vara)?;

        router.lookup_genesis_hash()?;

        for _ in 0..10 {
            router.commit_batch_simple(None, vec![], ExecutionMode::ExecuteAndCommit)?;
        }

        let validated_code_id = router.request_code_validation(b"code1")?;
        router.commit_batch_simple(
            None,
            vec![CodeCommitment {
                id: validated_code_id,
                valid: true,
            }],
            ExecutionMode::ExecuteAndCommit,
        )?;

        let validation_requested_code_id = router.request_code_validation(b"code2")?;

        let uninitialized_actor_id =
            router.create_program(validated_code_id, H256([0x01; 32]), None)?;

        let journal = router.context().evm().journal_mut();
        journal.balance_incr(
            uninitialized_actor_id.to_address_lossy().0.into(),
            u64::MAX.try_into().expect("infallible"),
        )?;
        let state = journal.finalize();
        router.context().evm().commit(state);

        let initialized_actor_id =
            router.create_program(validated_code_id, H256([0x02; 32]), None)?;

        let journal = router.context().evm().journal_mut();
        journal.balance_incr(
            initialized_actor_id.to_address_lossy().0.into(),
            u64::MAX.try_into().expect("infallible"),
        )?;
        let state = journal.finalize();
        router.context().evm().commit(state);

        let state_transition = Self::state_transition(initialized_actor_id);
        let head_announce = Self::head_announce();

        router.commit_batch_simple(
            Some(ChainCommitment {
                transitions: vec![state_transition.clone()],
                head_announce,
            }),
            vec![],
            ExecutionMode::ExecuteAndCommit,
        )?;

        Ok(InitializedSimulationContext {
            router,
            validated_code_id,
            validation_requested_code_id,
            uninitialized_actor_id,
            initialized_actor_id,
        })
    }

    pub fn evm(
        &mut self,
    ) -> &mut MainnetEvm<Context<BlockEnv, TxEnv, CfgEnv, CacheDB<EmptyDB>>, SimulationInspector>
    {
        &mut self.evm
    }

    pub fn block_number(&self) -> U256 {
        self.block_number
    }

    fn block_timestamp(&self) -> U256 {
        self.block_timestamp
    }

    pub fn block_timestamp_u64(&self) -> u64 {
        self.block_timestamp().try_into().expect("infallible")
    }

    pub fn block_hash(&self, number: U256) -> Result<H256> {
        Ok(self
            .evm
            .ctx
            .db_ref()
            .block_hash_ref(number.try_into().expect("infallible"))?
            .0
            .into())
    }

    pub fn parent_block_hash(&self) -> Result<H256> {
        self.block_hash(
            self.block_number
                .checked_sub(U256::ONE)
                .expect("infallible"),
        )
    }

    pub fn parent_block_timestamp_u64(&self) -> u64 {
        self.block_timestamp_u64()
            .checked_sub(1)
            .expect("infallible")
    }

    pub fn next_block(&mut self) {
        self.evm.modify_block(|block_env| {
            let one = U256::ONE;

            self.block_number += one;
            block_env.number += one;

            self.block_timestamp += one;
            block_env.timestamp += one;
        });
    }

    pub fn prev_block(&mut self) {
        self.evm.modify_block(|block_env| {
            let one = U256::ONE;

            if self.block_number > U256::ZERO {
                self.block_number -= one;
                block_env.number -= one;
            }

            if self.block_timestamp > U256::ZERO {
                self.block_timestamp -= one;
                block_env.timestamp -= one;
            }
        });
    }

    pub fn deployer_address(&self) -> Address {
        self.deployer_address
    }

    pub fn deployer_nonce(&self) -> u64 {
        self.deployer_nonce
    }

    pub fn increment_deployer_nonce(&mut self) {
        self.deployer_nonce += 1;
    }

    pub fn validators_with_keys(&self) -> &[(Signer, PublicKey, Address)] {
        &self.validators_with_keys
    }

    pub fn validators(&self) -> Vec<Address> {
        self.validators_with_keys
            .iter()
            .map(|(_, _, address)| *address)
            .collect()
    }

    pub fn min_signers(&self) -> u16 {
        self.max_signers()
            .checked_mul(2)
            .expect("multiplication failed")
            .div_ceil(3)
    }

    fn max_signers(&self) -> u16 {
        self.validators_with_keys
            .len()
            .try_into()
            .expect("conversion failed")
    }

    fn state_transition(actor_id: ActorId) -> StateTransition {
        StateTransition {
            actor_id,
            new_state_hash: H256::random(),
            exited: false,
            inheritor: ActorId::zero(),
            value_to_receive: 0,
            value_to_receive_negative_sign: false,
            value_claims: vec![],
            messages: vec![],
        }
    }

    fn head_announce() -> HashOf<Announce> {
        unsafe { HashOf::new(H256([0x01; 32])) }
    }
}

pub struct InitializedSimulationContext<'a> {
    router: Router<'a>,
    validated_code_id: CodeId,
    validation_requested_code_id: CodeId,
    uninitialized_actor_id: ActorId,
    initialized_actor_id: ActorId,
}

impl<'a> InitializedSimulationContext<'a> {
    pub fn uninitialized_actor_id(&self) -> ActorId {
        self.uninitialized_actor_id
    }

    pub fn initialized_actor_id(&self) -> ActorId {
        self.initialized_actor_id
    }

    pub fn switch_to_impl(&mut self, contract_impl_kind: ContractImplKind) -> Result<()> {
        self.router.switch_to_impl(contract_impl_kind)
    }

    pub fn empty_batch_gas(&mut self) -> Result<u64> {
        let empty_batch_gas = self
            .router
            .estimate_commit_batch_gas(None, vec![], ExecutionMode::Execute)?
            .total_tx_gas();
        Ok(empty_batch_gas)
    }

    pub fn code_commitment_gas(&mut self) -> Result<u64> {
        const COMMIT_BATCH_BEFORE_COMMIT_CODES: u32 = 1;
        const COMMIT_BATCH_AFTER_COMMIT_CODES: u32 = 2;

        let code_commitment = CodeCommitment {
            id: self.validation_requested_code_id,
            valid: true,
        };
        let alloy_code_commitment: Gear::CodeCommitment = code_commitment.clone().into();
        let calldata: Bytes = alloy_code_commitment.abi_encode().into();
        let calldata_gas = calldata.calldata_gas().total_gas();

        let code_commitment_gas = self.router.estimate_commit_batch_gas_between_topics(
            None,
            vec![code_commitment],
            self.router.proxy_address(),
            COMMIT_BATCH_BEFORE_COMMIT_CODES,
            COMMIT_BATCH_AFTER_COMMIT_CODES,
        )?;
        Ok(code_commitment_gas
            .execution_gas
            .checked_add(calldata_gas)
            .expect("infallible"))
    }

    fn state_transition(actor_id: ActorId) -> StateTransition {
        StateTransition {
            actor_id,
            new_state_hash: H256::random(),
            exited: false,
            inheritor: ActorId::zero(),
            value_to_receive: 0,
            value_to_receive_negative_sign: false,
            value_claims: vec![],
            messages: vec![],
        }
    }

    fn head_announce() -> HashOf<Announce> {
        unsafe { HashOf::new(H256([0x01; 32])) }
    }

    pub fn state_transition_actor_id_gas(&mut self, actor_id: ActorId) -> u64 {
        let alloy_actor_id: Address = actor_id.into();
        let calldata: Bytes = alloy_actor_id.abi_encode().into();
        calldata.calldata_gas().total_gas()
    }

    pub fn state_transition_new_state_hash_gas(&mut self, new_state_hash: H256) -> u64 {
        let alloy_new_state_hash = utils::h256_to_bytes32(new_state_hash);
        let calldata: Bytes = alloy_new_state_hash.abi_encode().into();
        calldata.calldata_gas().total_gas()
    }

    pub fn state_transition_exited_gas(&mut self, exited: bool) -> u64 {
        let calldata: Bytes = exited.abi_encode().into();
        calldata.calldata_gas().total_gas()
    }

    pub fn state_transition_inheritor_gas(&mut self, inheritor: ActorId) -> u64 {
        let alloy_inheritor: Address = inheritor.into();
        let calldata: Bytes = alloy_inheritor.abi_encode().into();
        calldata.calldata_gas().total_gas()
    }

    pub fn state_transition_value_to_receive_gas(&mut self, value_to_receive: u128) -> u64 {
        let calldata: Bytes = value_to_receive.abi_encode().into();
        calldata.calldata_gas().total_gas()
    }

    pub fn state_transition_value_to_receive_negative_sign_gas(
        &mut self,
        value_to_receive_negative_sign: bool,
    ) -> u64 {
        let calldata: Bytes = value_to_receive_negative_sign.abi_encode().into();
        calldata.calldata_gas().total_gas()
    }

    pub fn state_transition_value_claims_gas(&mut self, value_claims: Vec<ValueClaim>) -> u64 {
        let alloy_value_claims: Vec<Gear::ValueClaim> =
            value_claims.into_iter().map(Into::into).collect();
        let calldata: Bytes = alloy_value_claims.abi_encode().into();
        calldata.calldata_gas().total_gas()
    }

    pub fn state_transition_messages_gas(&mut self, messages: Vec<Message>) -> u64 {
        let alloy_messages: Vec<Gear::Message> = messages.into_iter().map(Into::into).collect();
        let calldata: Bytes = alloy_messages.abi_encode().into();
        calldata.calldata_gas().total_gas()
    }

    pub fn verify_actor_id_gas(&mut self) -> Result<u64> {
        const PERFORM_STATE_TRANSITION_BEFORE_VERIFY_ACTOR_ID: u32 = 1;
        const PERFORM_STATE_TRANSITION_AFTER_VERIFY_ACTOR_ID: u32 = 2;

        let verify_actor_id_gas = self.router.estimate_commit_batch_gas_between_topics(
            Some(ChainCommitment {
                transitions: vec![Self::state_transition(self.initialized_actor_id)],
                head_announce: Self::head_announce(),
            }),
            vec![],
            self.initialized_actor_id.into(),
            PERFORM_STATE_TRANSITION_BEFORE_VERIFY_ACTOR_ID,
            PERFORM_STATE_TRANSITION_AFTER_VERIFY_ACTOR_ID,
        )?;
        Ok(verify_actor_id_gas.execution_gas)
    }

    pub fn retrieve_ether_gas(
        &mut self,
        value_to_receive: u128,
        value_to_receive_negative_sign: bool,
    ) -> Result<u64> {
        const PERFORM_STATE_TRANSITION_BEFORE_RETRIEVE_ETHER: u32 = 3;
        const PERFORM_STATE_TRANSITION_AFTER_RETRIEVE_ETHER: u32 = 4;

        let retrieve_ether_gas = self.router.estimate_commit_batch_gas_between_topics(
            Some(ChainCommitment {
                transitions: vec![StateTransition {
                    value_to_receive,
                    value_to_receive_negative_sign,
                    ..Self::state_transition(self.initialized_actor_id)
                }],
                head_announce: Self::head_announce(),
            }),
            vec![],
            self.initialized_actor_id.into(),
            PERFORM_STATE_TRANSITION_BEFORE_RETRIEVE_ETHER,
            PERFORM_STATE_TRANSITION_AFTER_RETRIEVE_ETHER,
        )?;
        Ok(retrieve_ether_gas.execution_gas)
    }

    pub fn send_message_gas(&mut self, message: Message) -> Result<u64> {
        const PERFORM_STATE_TRANSITION_BEFORE_SEND_MESSAGES: u32 = 5;
        const PERFORM_STATE_TRANSITION_AFTER_SEND_MESSAGES: u32 = 6;

        let alloy_message: Gear::Message = message.clone().into();
        let calldata: Bytes = alloy_message.abi_encode().into();
        let calldata_gas = calldata.calldata_gas().total_gas();

        let send_message_gas = self.router.estimate_commit_batch_gas_between_topics(
            Some(ChainCommitment {
                transitions: vec![StateTransition {
                    messages: vec![message.clone()],
                    ..Self::state_transition(self.initialized_actor_id)
                }],
                head_announce: Self::head_announce(),
            }),
            vec![],
            self.initialized_actor_id.into(),
            PERFORM_STATE_TRANSITION_BEFORE_SEND_MESSAGES,
            PERFORM_STATE_TRANSITION_AFTER_SEND_MESSAGES,
        )?;

        if message.call {
            const CALL_GAS: u64 = 500_000;

            Ok(send_message_gas
                .execution_gas
                .checked_add(calldata_gas)
                .expect("infallible")
                .checked_add(CALL_GAS)
                .expect("infallible"))
        } else {
            Ok(send_message_gas
                .execution_gas
                .checked_add(calldata_gas)
                .expect("infallible"))
        }
    }

    pub fn value_claim_gas(&mut self, value_claim: ValueClaim) -> Result<u64> {
        const PERFORM_STATE_TRANSITION_BEFORE_CLAIM_VALUES: u32 = 7;
        const PERFORM_STATE_TRANSITION_AFTER_CLAIM_VALUES: u32 = 8;

        let alloy_value_claim: Gear::ValueClaim = value_claim.clone().into();
        let calldata: Bytes = alloy_value_claim.abi_encode().into();
        let calldata_gas = calldata.calldata_gas().total_gas();

        let value_claim_gas = self.router.estimate_commit_batch_gas_between_topics(
            Some(ChainCommitment {
                transitions: vec![StateTransition {
                    value_claims: vec![value_claim.clone()],
                    ..Self::state_transition(self.initialized_actor_id)
                }],
                head_announce: Self::head_announce(),
            }),
            vec![],
            self.initialized_actor_id.into(),
            PERFORM_STATE_TRANSITION_BEFORE_CLAIM_VALUES,
            PERFORM_STATE_TRANSITION_AFTER_CLAIM_VALUES,
        )?;

        Ok(value_claim_gas
            .execution_gas
            .checked_add(calldata_gas)
            .expect("infallible"))
    }

    pub fn set_inheritor_gas(&mut self, inheritor: Option<ActorId>) -> Result<u64> {
        const PERFORM_STATE_TRANSITION_BEFORE_SET_INHERITOR: u32 = 9;
        const PERFORM_STATE_TRANSITION_AFTER_SET_INHERITOR: u32 = 10;

        let set_inheritor_gas = self.router.estimate_commit_batch_gas_between_topics(
            Some(ChainCommitment {
                transitions: vec![StateTransition {
                    exited: inheritor.is_some(),
                    inheritor: inheritor.unwrap_or_default(),
                    ..Self::state_transition(self.initialized_actor_id)
                }],
                head_announce: Self::head_announce(),
            }),
            vec![],
            self.initialized_actor_id.into(),
            PERFORM_STATE_TRANSITION_BEFORE_SET_INHERITOR,
            PERFORM_STATE_TRANSITION_AFTER_SET_INHERITOR,
        )?;

        Ok(set_inheritor_gas.execution_gas)
    }

    pub fn update_state_hash_gas(
        &mut self,
        actor_id: ActorId,
        new_state_hash: H256,
    ) -> Result<u64> {
        const PERFORM_STATE_TRANSITION_BEFORE_UPDATE_STATE_HASH: u32 = 11;
        const PERFORM_STATE_TRANSITION_AFTER_UPDATE_STATE_HASH: u32 = 12;

        let update_state_hash_gas = self.router.estimate_commit_batch_gas_between_topics(
            Some(ChainCommitment {
                transitions: vec![StateTransition {
                    new_state_hash,
                    ..Self::state_transition(actor_id)
                }],
                head_announce: Self::head_announce(),
            }),
            vec![],
            actor_id.into(),
            PERFORM_STATE_TRANSITION_BEFORE_UPDATE_STATE_HASH,
            PERFORM_STATE_TRANSITION_AFTER_UPDATE_STATE_HASH,
        )?;

        Ok(update_state_hash_gas.execution_gas)
    }

    pub fn state_transition_hash_gas(&mut self) -> Result<u64> {
        const PERFORM_STATE_TRANSITION_BEFORE_RETURN_HASH: u32 = 13;
        const PERFORM_STATE_TRANSITION_AFTER_RETURN_HASH: u32 = 14;

        let state_transition_hash_gas = self.router.estimate_commit_batch_gas_between_topics(
            Some(ChainCommitment {
                transitions: vec![Self::state_transition(self.initialized_actor_id)],
                head_announce: Self::head_announce(),
            }),
            vec![],
            self.initialized_actor_id.into(),
            PERFORM_STATE_TRANSITION_BEFORE_RETURN_HASH,
            PERFORM_STATE_TRANSITION_AFTER_RETURN_HASH,
        )?;

        Ok(state_transition_hash_gas.execution_gas)
    }
}
