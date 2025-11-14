// This file is part of Gear.
//
// Copyright (C) 2025 Gear Technologies Inc.
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

use super::{core::*, *};
use anyhow::anyhow;
use async_trait::async_trait;
use ethexe_common::{
    DEFAULT_BLOCK_GAS_LIMIT, ProtocolTimelines, ValidatorsVec, db::OnChainStorageRW,
    ecdsa::ContractSignature, gear::BatchCommitment, mock::Mock,
};
use hashbrown::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

type BatchWithSignatures = (BatchCommitment, Vec<ContractSignature>);

#[derive(Default, Clone)]
pub struct MockEthereum {
    pub committed_batch: Arc<RwLock<Option<BatchWithSignatures>>>,
    pub predefined_election_at: Arc<RwLock<HashMap<u64, ValidatorsVec>>>,
}

#[async_trait]
impl BatchCommitter for MockEthereum {
    fn clone_boxed(&self) -> Box<dyn BatchCommitter> {
        Box::new(self.clone())
    }

    async fn commit(
        self: Box<Self>,
        batch: BatchCommitment,
        signatures: Vec<ContractSignature>,
    ) -> Result<H256> {
        self.committed_batch
            .write()
            .await
            .replace((batch, signatures));
        Ok(H256::random())
    }
}

#[async_trait]
impl ElectionProvider for MockEthereum {
    fn clone_boxed(&self) -> Box<dyn ElectionProvider> {
        Box::new(self.clone())
    }

    async fn make_election_at(&self, ts: u64, _max_validators: u128) -> Result<ValidatorsVec> {
        match self.predefined_election_at.read().await.get(&ts) {
            Some(election_result) => Ok(election_result.clone()),
            None => Err(anyhow!(
                "No predefined election result for the given request"
            )),
        }
    }
}

#[async_trait]
pub trait WaitFor {
    async fn wait_for_event(self) -> Result<(ValidatorState, ConsensusEvent)>;
    async fn wait_for_state<F>(self, f: F) -> Result<ValidatorState>
    where
        F: Fn(&ValidatorState) -> bool + Unpin + Send;
}

#[async_trait]
impl WaitFor for ValidatorState {
    async fn wait_for_event(self) -> Result<(ValidatorState, ConsensusEvent)> {
        struct Dummy(Option<ValidatorState>);

        impl Future for Dummy {
            type Output = Result<ConsensusEvent>;

            fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
                let mut event;
                loop {
                    let (poll, mut state) = self.0.take().unwrap().poll_next_state(cx)?;
                    event = state.context_mut().output.pop_front();
                    self.0 = Some(state);

                    if poll.is_pending() || event.is_some() {
                        break;
                    }
                }

                event.map(|e| Poll::Ready(Ok(e))).unwrap_or(Poll::Pending)
            }
        }

        let mut dummy = Dummy(Some(self));
        (&mut dummy).await.map(|event| (dummy.0.unwrap(), event))
    }

    async fn wait_for_state<F>(self, f: F) -> Result<ValidatorState>
    where
        F: Fn(&ValidatorState) -> bool + Unpin + Send,
    {
        struct Dummy<F>(Option<ValidatorState>, F);

        impl<F> Future for Dummy<F>
        where
            F: Fn(&ValidatorState) -> bool + Unpin + Send,
        {
            type Output = Result<ValidatorState>;

            fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
                loop {
                    let (poll, state) = self.0.take().unwrap().poll_next_state(cx)?;

                    if self.1(&state) {
                        return Poll::Ready(Ok(state));
                    }

                    self.0 = Some(state);

                    if poll.is_pending() {
                        break;
                    }
                }

                Poll::Pending
            }
        }

        let mut dummy = Dummy(Some(self), f);
        (&mut dummy).await
    }
}

pub fn mock_validator_context() -> (ValidatorContext, Vec<PublicKey>, MockEthereum) {
    let (signer, _, mut keys) = crate::mock::init_signer_with_keys(10);
    let ethereum = MockEthereum::default();
    let db = Database::memory();
    let timelines = ProtocolTimelines::mock(());

    let ctx = ValidatorContext {
        core: ValidatorCore {
            slot_duration: Duration::from_secs(1),
            signatures_threshold: 1,
            router_address: 12345.into(),
            pub_key: keys.pop().unwrap(),
            timelines,
            block_gas_limit: DEFAULT_BLOCK_GAS_LIMIT,
            signer,
            db: db.clone(),
            committer: Box::new(ethereum.clone()),
            middleware: MiddlewareWrapper::from_inner(ethereum.clone()),
            validate_chain_deepness_limit: MAX_CHAIN_DEEPNESS,
            chain_deepness_threshold: CHAIN_DEEPNESS_THRESHOLD,
            commitment_delay_limit: 3,
            producer_delay: Duration::from_millis(1),
        },
        pending_events: VecDeque::new(),
        output: VecDeque::new(),
        tasks: Default::default(),
    };

    ctx.core.db.set_protocol_timelines(timelines);

    (ctx, keys, ethereum)
}
