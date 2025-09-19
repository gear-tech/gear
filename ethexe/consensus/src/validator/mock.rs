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
use crate::utils::MultisignedBatchCommitment;
use anyhow::anyhow;
use async_trait::async_trait;
use std::cell::RefCell;

thread_local! {
    static BATCH: RefCell<Option<MultisignedBatchCommitment>> = const { RefCell::new(None) };
    static ELECTION_RESULT: RefCell<Option<Vec<Address>>> = const { RefCell::new(None) };
}

pub fn with_batch(f: impl FnOnce(Option<&MultisignedBatchCommitment>)) {
    BATCH.with_borrow(|storage| f(storage.as_ref()));
}

struct DummyCommitter;

#[async_trait]
impl BatchCommitter for DummyCommitter {
    fn clone_boxed(&self) -> Box<dyn BatchCommitter> {
        Box::new(DummyCommitter)
    }

    async fn commit_batch(self: Box<Self>, batch: MultisignedBatchCommitment) -> Result<H256> {
        BATCH.with_borrow_mut(|storage| storage.replace(batch));
        Ok(H256::random())
    }
}

struct DummyMiddleware;

#[async_trait]
impl MiddlewareExt for DummyMiddleware {
    fn clone_boxed(&self) -> Box<dyn MiddlewareExt> {
        Box::new(DummyMiddleware)
    }

    async fn make_election_at(self: Box<Self>, _request: ElectionRequest) -> Result<Vec<Address>> {
        ELECTION_RESULT.with_borrow_mut(|storage| {
            if let Some(result) = &*storage {
                Ok(result.clone())
            } else {
                Err(anyhow!("No cached election result"))
            }
        })
    }
}

#[async_trait]
pub trait WaitFor {
    async fn wait_for_event(self) -> Result<(ValidatorState, ConsensusEvent)>;
    async fn wait_for_initial(self) -> Result<ValidatorState>;
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
        let event = (&mut dummy).await?;
        Ok((dummy.0.unwrap(), event))
    }

    async fn wait_for_initial(self) -> Result<ValidatorState> {
        struct Dummy(Option<ValidatorState>);

        impl Future for Dummy {
            type Output = Result<ValidatorState>;

            fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
                loop {
                    let (poll, state) = self.0.take().unwrap().poll_next_state(cx)?;

                    if state.is_initial() {
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

        let mut dummy = Dummy(Some(self));
        (&mut dummy).await
    }
}

pub fn mock_validator_context() -> (ValidatorContext, Vec<PublicKey>) {
    let (signer, _, mut keys) = crate::mock::init_signer_with_keys(10);

    let ctx = ValidatorContext {
        core: ValidatorCore {
            slot_duration: Duration::from_secs(1),
            signatures_threshold: 1,
            router_address: 12345.into(),
            pub_key: keys.pop().unwrap(),
            signer,
            db: Database::memory(),
            committer: Box::new(DummyCommitter),
            middleware: Some(Box::new(DummyMiddleware) as Box<dyn MiddlewareExt>),
            validate_chain_deepness_limit: MAX_CHAIN_DEEPNESS,
            chain_deepness_threshold: CHAIN_DEEPNESS_THRESHOLD,
        },
        pending_events: VecDeque::new(),
        output: VecDeque::new(),
    };

    (ctx, keys)
}
