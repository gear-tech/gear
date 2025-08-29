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

use super::*;
use ethexe_common::DEFAULT_BLOCK_GAS_LIMIT;
use std::cell::RefCell;

thread_local! {
    static BATCH: RefCell<Option<MultisignedBatchCommitment>> = const { RefCell::new(None) };
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

#[async_trait]
pub trait WaitForEvent {
    async fn wait_for_event(self) -> Result<(ValidatorState, ConsensusEvent)>;
}

#[async_trait]
impl WaitForEvent for ValidatorState {
    async fn wait_for_event(self) -> Result<(ValidatorState, ConsensusEvent)> {
        wait_for_event_inner(self).await
    }
}

pub fn mock_validator_context() -> (ValidatorContext, Vec<PublicKey>) {
    let (signer, _, mut keys) = crate::mock::init_signer_with_keys(10);

    let ctx = ValidatorContext {
        slot_duration: Duration::from_secs(1),
        signatures_threshold: 1,
        router_address: 12345.into(),
        pub_key: keys.pop().unwrap(),
        block_gas_limit: DEFAULT_BLOCK_GAS_LIMIT,
        signer,
        db: Database::memory(),
        committer: Box::new(DummyCommitter),
        pending_events: VecDeque::new(),
        output: VecDeque::new(),
    };

    (ctx, keys)
}

async fn wait_for_event_inner(s: ValidatorState) -> Result<(ValidatorState, ConsensusEvent)> {
    struct Dummy(Option<ValidatorState>);

    impl Future for Dummy {
        type Output = Result<ConsensusEvent>;

        fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            let mut s = self.0.take().unwrap().poll_next_state(cx)?;
            let res = s
                .context_mut()
                .output
                .pop_front()
                .map(|event| Poll::Ready(Ok(event)))
                .unwrap_or(Poll::Pending);
            self.0 = Some(s);
            res
        }
    }

    let mut dummy = Dummy(Some(s));
    let event = (&mut dummy).await?;
    Ok((dummy.0.unwrap(), event))
}
