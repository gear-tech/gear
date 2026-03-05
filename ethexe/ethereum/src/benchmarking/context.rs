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

use crate::benchmarking::{
    ExecutionMode, Router,
    contracts::{MirrorImplKind, RouterImplKind, WrappedVara},
    inspector::SimulationInspector,
    mnemonic,
};
use anyhow::Result;
use ethexe_common::{
    HashOf,
    gear::{ChainCommitment, CodeCommitment, StateTransition},
};
use gprimitives::{ActorId, H256};
use gsigner::secp256k1::{PublicKey, Signer};
use revm::{
    DatabaseRef, ExecuteCommitEvm, MainBuilder, MainContext, MainnetEvm,
    context::{BlockEnv, CfgEnv, Context, ContextTr, JournalTr, TxEnv},
    database::CacheDB,
    database_interface::EmptyDB,
    primitives::{Address, U256},
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

    pub fn deploy(&mut self) -> Result<()> {
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

        let id = router.request_code_validation(b"code1")?;
        router.commit_batch_simple(
            None,
            vec![CodeCommitment { id, valid: true }],
            ExecutionMode::ExecuteAndCommit,
        )?;

        let uninitialized_actor_id = router.create_program(id, H256([0x01; 32]), None)?;

        let journal = router.context.evm.journal_mut();
        journal.balance_incr(
            uninitialized_actor_id.to_address_lossy().0.into(),
            u64::MAX.try_into().expect("infallible"),
        )?;
        let state = journal.finalize();
        router.context.evm.commit(state);

        let initialized_actor_id = router.create_program(id, H256([0x02; 32]), None)?;

        let journal = router.context.evm.journal_mut();
        journal.balance_incr(
            initialized_actor_id.to_address_lossy().0.into(),
            u64::MAX.try_into().expect("infallible"),
        )?;
        let state = journal.finalize();
        router.context.evm.commit(state);

        let state_transition = StateTransition {
            actor_id: initialized_actor_id,
            new_state_hash: H256::random(),
            exited: false,
            inheritor: ActorId::zero(),
            value_to_receive: 0,
            value_to_receive_negative_sign: false,
            value_claims: vec![],
            messages: vec![],
        };
        let head_announce = unsafe { HashOf::new(H256([0x01; 32])) };

        router.commit_batch_simple(
            Some(ChainCommitment {
                transitions: vec![state_transition.clone()],
                head_announce,
            }),
            vec![],
            ExecutionMode::ExecuteAndCommit,
        )?;

        //

        router.switch_to_mirror_impl(MirrorImplKind::Regular)?;
        router.switch_to_router_impl(RouterImplKind::Regular)?;

        let empty_batch_gas =
            router.estimate_commit_batch_gas(None, vec![], ExecutionMode::Execute)?;
        dbg!(empty_batch_gas);

        //

        let id = router.request_code_validation(b"code2")?;
        let code_commitment_gas = router.estimate_commit_batch_gas(
            None,
            vec![CodeCommitment { id, valid: true }],
            ExecutionMode::Execute,
        )?;
        dbg!(code_commitment_gas);

        //

        router.switch_to_mirror_impl(MirrorImplKind::WithInstrumentation)?;
        router.switch_to_router_impl(RouterImplKind::WithInstrumentation)?;

        const COMMIT_BATCH_BEFORE_COMMIT_CODES: u32 = 1;
        const COMMIT_BATCH_AFTER_COMMIT_CODES: u32 = 2;

        let id = router.request_code_validation(b"code3")?;
        let code_commitment_gas = router.estimate_commit_batch_gas_between_topics(
            None,
            vec![CodeCommitment { id, valid: true }],
            router.proxy_address(),
            COMMIT_BATCH_BEFORE_COMMIT_CODES,
            COMMIT_BATCH_AFTER_COMMIT_CODES,
        )?;
        dbg!(code_commitment_gas);

        //

        const PERFORM_STATE_TRANSITION_BEFORE_VERIFY_ACTOR_ID: u32 = 1;
        const PERFORM_STATE_TRANSITION_AFTER_VERIFY_ACTOR_ID: u32 = 2;

        let verify_actor_id_gas = router.estimate_commit_batch_gas_between_topics(
            Some(ChainCommitment {
                transitions: vec![state_transition.clone()],
                head_announce,
            }),
            vec![],
            initialized_actor_id.into(),
            PERFORM_STATE_TRANSITION_BEFORE_VERIFY_ACTOR_ID,
            PERFORM_STATE_TRANSITION_AFTER_VERIFY_ACTOR_ID,
        )?;
        dbg!(verify_actor_id_gas.execution_gas);

        //

        const PERFORM_STATE_TRANSITION_BEFORE_RETRIEVE_ETHER: u32 = 3;
        const PERFORM_STATE_TRANSITION_AFTER_RETRIEVE_ETHER: u32 = 4;

        let retrieve_ether_gas1 = router.estimate_commit_batch_gas_between_topics(
            Some(ChainCommitment {
                transitions: vec![StateTransition {
                    ..state_transition.clone()
                }],
                head_announce,
            }),
            vec![],
            initialized_actor_id.into(),
            PERFORM_STATE_TRANSITION_BEFORE_RETRIEVE_ETHER,
            PERFORM_STATE_TRANSITION_AFTER_RETRIEVE_ETHER,
        )?;
        dbg!(retrieve_ether_gas1.execution_gas);

        //

        let retrieve_ether_gas2 = router.estimate_commit_batch_gas_between_topics(
            Some(ChainCommitment {
                transitions: vec![StateTransition {
                    value_to_receive_negative_sign: true,
                    ..state_transition.clone()
                }],
                head_announce,
            }),
            vec![],
            initialized_actor_id.into(),
            PERFORM_STATE_TRANSITION_BEFORE_RETRIEVE_ETHER,
            PERFORM_STATE_TRANSITION_AFTER_RETRIEVE_ETHER,
        )?;
        dbg!(retrieve_ether_gas2.execution_gas);

        //

        let retrieve_ether_gas3 = router.estimate_commit_batch_gas_between_topics(
            Some(ChainCommitment {
                transitions: vec![StateTransition {
                    value_to_receive: 1,
                    value_to_receive_negative_sign: true,
                    ..state_transition.clone()
                }],
                head_announce,
            }),
            vec![],
            initialized_actor_id.into(),
            PERFORM_STATE_TRANSITION_BEFORE_RETRIEVE_ETHER,
            PERFORM_STATE_TRANSITION_AFTER_RETRIEVE_ETHER,
        )?;
        dbg!(retrieve_ether_gas3.execution_gas);

        Ok(())
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
}
