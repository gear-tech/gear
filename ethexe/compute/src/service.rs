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

use crate::{
    ComputeError, ComputeEvent, ProcessorExt, Result,
    compute::{self, ComputationStatus},
    prepare::{self, MissingData},
};
use ethexe_common::{
    AnnounceHash, AnnouncesRequest, BlockMetaStorageRead, CodeAndIdUnchecked, CodesStorageRead,
    DataRequest, ProducerBlock,
};
use ethexe_db::Database;
use futures::{FutureExt, Stream, future::BoxFuture, stream::FusedStream};
use gprimitives::{CodeId, H256};
use std::{
    collections::VecDeque,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::task::JoinSet;

#[derive(Debug, Clone)]
pub struct ComputeMetrics {
    pub blocks_queue_len: usize,
    pub process_codes_count: usize,
    pub waiting_for_requests: Vec<DataRequest>,
}

#[derive(Debug, Clone)]
enum BlockAction {
    Prepare(H256),
    Process(ProducerBlock),
}

#[derive(Default)]
enum State {
    #[default]
    WaitForBlock,
    WaitForRequestedData {
        block_hash: H256,
        requests: Vec<DataRequest>,
    },
    Preparation {
        block_hash: H256,
        future: BoxFuture<'static, Result<()>>,
    },
    Computation {
        announce_hash: AnnounceHash,
        future: BoxFuture<'static, Result<ComputationStatus>>,
    },
}

pub struct ComputeService<P: ProcessorExt> {
    db: Database,
    processor: P,

    blocks_queue: VecDeque<BlockAction>,
    blocks_state: State,

    process_codes: JoinSet<Result<CodeId>>,
}

impl<P: ProcessorExt> ComputeService<P> {
    // TODO #4550: consider to create Processor inside ComputeService
    pub fn new(db: Database, processor: P) -> Self {
        Self {
            db,
            processor,
            blocks_queue: Default::default(),
            blocks_state: State::WaitForBlock,
            process_codes: Default::default(),
        }
    }

    pub fn process_code(&mut self, code_and_id: CodeAndIdUnchecked) {
        let code_id = code_and_id.code_id;
        if let Some(valid) = self.db.code_valid(code_id) {
            // TODO: #4712 test this case
            log::warn!("Code {code_id:?} already processed");

            if valid {
                debug_assert!(
                    self.db.original_code_exists(code_id),
                    "Code {code_id:?} must exist in database"
                );
                debug_assert!(
                    self.db
                        .instrumented_code_exists(ethexe_runtime::VERSION, code_id),
                    "Instrumented code {code_id:?} must exist in database"
                );
            }

            self.process_codes.spawn(async move { Ok(code_id) });
        } else {
            let mut processor = self.processor.clone();

            self.process_codes.spawn_blocking(move || {
                processor
                    .process_upload_code(code_and_id)
                    .map(|_valid| code_id)
            });
        }
    }

    pub fn prepare_block(&mut self, block: H256) {
        self.blocks_queue.push_front(BlockAction::Prepare(block));
    }

    pub fn compute_announce(&mut self, announce: ProducerBlock) {
        self.blocks_queue.push_front(BlockAction::Process(announce));
    }

    pub fn receive_requested_announces(
        &mut self,
        _block: H256,
        _announces_request: (AnnounceHash, u32),
    ) -> Result<()> {
        todo!("TODO +_+_+: implement receive_requested_announces");
    }

    /// Get all metrics from the compute service
    pub fn get_metrics(&self) -> ComputeMetrics {
        let waiting_for_requests =
            if let State::WaitForRequestedData { requests, .. } = &self.blocks_state {
                requests.clone()
            } else {
                Vec::new()
            };

        ComputeMetrics {
            blocks_queue_len: self.blocks_queue.len(),
            process_codes_count: self.process_codes.len(),
            waiting_for_requests,
        }
    }
}

impl<P: ProcessorExt> Stream for ComputeService<P> {
    type Item = Result<ComputeEvent>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if let Poll::Ready(Some(res)) = self.process_codes.poll_join_next(cx) {
            match res {
                Ok(res) => {
                    if let (Ok(code_id), State::WaitForRequestedData { requests, .. }) =
                        (&res, &mut self.blocks_state)
                    {
                        let mut ready_request_positions = Vec::new();
                        for (pos, request) in requests.iter_mut().enumerate() {
                            if let DataRequest::Codes(codes) = request {
                                codes.remove(code_id);
                                if codes.is_empty() {
                                    ready_request_positions.push(pos);
                                }
                            }
                        }

                        // Remove requests that are now empty
                        for pos in ready_request_positions.into_iter().rev() {
                            requests.remove(pos);
                        }
                    }

                    return Poll::Ready(Some(res.map(ComputeEvent::CodeProcessed)));
                }
                Err(e) => return Poll::Ready(Some(Err(ComputeError::CodeProcessJoin(e)))),
            }
        }

        if let State::WaitForBlock = &self.blocks_state {
            match self.blocks_queue.pop_back() {
                Some(BlockAction::Prepare(block)) => {
                    let MissingData {
                        codes,
                        validated_codes,
                        announces_request,
                    } = prepare::missing_data(&self.db, block, 3)?;

                    debug_assert!(
                        validated_codes
                            .iter()
                            .all(|code_id| codes.contains(code_id)),
                        "All missing validated codes must be in the missing codes list"
                    );

                    let mut requests = vec![];
                    if !validated_codes.is_empty() {
                        requests.push(DataRequest::Codes(validated_codes.into_iter().collect()));
                    }
                    if let Some((head, deepness)) = announces_request {
                        requests.push(DataRequest::Announces(AnnouncesRequest { head, deepness }));
                    }

                    self.blocks_state = State::WaitForRequestedData {
                        block_hash: block,
                        requests,
                    };

                    if !codes.is_empty() {
                        return Poll::Ready(Some(Ok(ComputeEvent::RequestLoadCodes(codes))));
                    }
                }
                Some(BlockAction::Process(announce)) => {
                    if !self.db.block_meta(announce.block_hash).prepared {
                        return Poll::Ready(Some(Err(ComputeError::BlockNotPrepared(
                            announce.block_hash,
                        ))));
                    }

                    self.blocks_state = State::Computation {
                        announce_hash: announce.hash(),
                        future: compute::compute(self.db.clone(), self.processor.clone(), announce)
                            .boxed(),
                    };
                }
                None => {}
            }
        }

        if let State::WaitForRequestedData {
            block_hash,
            requests,
        } = &self.blocks_state
            && requests.is_empty()
        {
            self.blocks_state = State::Preparation {
                block_hash: *block_hash,
                future: prepare::prepare(self.db.clone(), self.processor.clone(), *block_hash, 3)
                    .boxed(),
            };
        }

        if let State::Preparation { block_hash, future } = &mut self.blocks_state
            && let Poll::Ready(res) = future.poll_unpin(cx)
        {
            let result = res.map(|_| ComputeEvent::BlockPrepared(*block_hash));
            self.blocks_state = State::WaitForBlock;
            return Poll::Ready(Some(result));
        }

        if let State::Computation {
            announce_hash,
            future,
        } = &mut self.blocks_state
        {
            if let Poll::Ready(res) = future.poll_unpin(cx) {
                let announce_hash = *announce_hash;
                self.blocks_state = State::WaitForBlock;
                return Poll::Ready(Some(res.map(|status| match status {
                    ComputationStatus::Computed => ComputeEvent::AnnounceComputed(announce_hash),
                    ComputationStatus::Rejected => ComputeEvent::AnnounceRejected(announce_hash),
                })));
            }
        }

        Poll::Pending
    }
}

impl<P: ProcessorExt> FusedStream for ComputeService<P> {
    fn is_terminated(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::MockProcessor;
    use ethexe_common::{
        AnnounceStorageRead, AnnounceStorageWrite, BlockHeader, CodeAndIdUnchecked,
        db::{BlockMeta, BlockMetaStorageWrite, OnChainStorageWrite},
    };
    use ethexe_db::Database as DB;
    use futures::StreamExt;
    use gear_core::ids::prelude::CodeIdExt;
    use gprimitives::{CodeId, H256};

    /// Test ComputeService block preparation functionality
    #[tokio::test]
    async fn prepare_block() {
        let db = DB::memory();
        let processor = MockProcessor;
        let mut service = ComputeService::new(db.clone(), processor);

        let parent_hash = H256::from([1; 32]);
        let block_hash = H256::from([2; 32]);

        // Setup parent block as prepared and with computed announce
        let parent_announce = ProducerBlock::base(parent_hash, Default::default());
        db.set_announce(parent_announce.clone());
        db.mutate_announce_meta(parent_announce.hash(), |meta| {
            meta.computed = true;
        });
        db.mutate_block_meta(parent_hash, |meta| {
            *meta = BlockMeta::default_prepared();
            meta.announces = Some(vec![parent_announce.hash()])
        });

        // Setup on chain data for not prepared
        let header = BlockHeader {
            height: 2,
            parent_hash,
            timestamp: 2000,
        };
        db.set_block_header(block_hash, header);
        db.set_block_events(block_hash, &[]);

        // Request block preparation
        service.prepare_block(block_hash);

        // Poll service to process the preparation request
        let event = service.next().await.unwrap().unwrap();
        assert_eq!(event, ComputeEvent::BlockPrepared(block_hash));

        // Verify block is marked as prepared in DB
        assert!(db.block_meta(block_hash).prepared);
    }

    /// Test ComputeService block processing functionality
    #[tokio::test]
    async fn compute_announce() {
        gear_utils::init_default_logger();

        let db = DB::memory();
        let processor = MockProcessor;
        let mut service = ComputeService::new(db.clone(), processor);

        let parent_hash = H256::from([1; 32]);
        let block_hash = H256::from([2; 32]);

        // Setup parent block and one computed announce inside
        let parent_announce = ProducerBlock::base(parent_hash, Default::default());
        db.set_announce(parent_announce.clone());
        db.mutate_announce_meta(parent_announce.hash(), |meta| {
            meta.computed = true;
        });
        db.mutate_block_meta(parent_hash, |meta| {
            *meta = BlockMeta::default_prepared();
            meta.announces = Some(vec![parent_announce.hash()])
        });

        // Setup and prepare block
        let header = BlockHeader {
            height: 2,
            parent_hash,
            timestamp: 2000,
        };
        db.set_block_header(block_hash, header);
        db.set_block_events(block_hash, &[]);
        service.prepare_block(block_hash);
        let event = service.next().await.unwrap().unwrap();
        assert_eq!(event, ComputeEvent::BlockPrepared(block_hash));

        // Request computation
        let announce = ProducerBlock {
            block_hash,
            parent: parent_announce.hash(),
            gas_allowance: Some(42),
            off_chain_transactions: vec![],
        };
        let announce_hash = announce.hash();
        service.compute_announce(announce);

        // Poll service to process the block
        let event = service.next().await.unwrap().unwrap();
        assert_eq!(event, ComputeEvent::AnnounceComputed(announce_hash));

        // Verify block is marked as computed in DB
        assert!(db.announce_meta(announce_hash).computed);
    }

    /// Test ComputeService code processing functionality
    #[tokio::test]
    async fn process_code() {
        let db = DB::memory();
        let processor = MockProcessor;
        let mut service = ComputeService::new(db.clone(), processor);

        // Create test code
        let code = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00]; // Simple WASM header
        let code_id = CodeId::generate(&code);

        let code_and_id = CodeAndIdUnchecked { code, code_id };

        // Verify code is not yet in DB
        assert!(db.code_valid(code_id).is_none());

        // Request code processing
        service.process_code(code_and_id);

        // Poll service to process the code
        let event = service.next().await.unwrap().unwrap();

        // Should receive CodeProcessed event with correct code_id
        match event {
            ComputeEvent::CodeProcessed(processed_code_id) => {
                assert_eq!(processed_code_id, code_id);
            }
            _ => panic!("Expected CodeProcessed event"),
        }
    }
}
