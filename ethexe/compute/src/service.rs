use crate::{
    compute,
    prepare::{self, PrepareInfo},
    BlockProcessed, ComputeError, ComputeEvent, Result,
};
use ethexe_common::{
    db::{BlockMetaStorageRead, BlockMetaStorageWrite, CodesStorageRead},
    CodeAndIdUnchecked, SimpleBlockData,
};
use ethexe_db::Database;
use ethexe_processor::Processor;
use futures::{future::BoxFuture, stream::FusedStream, FutureExt, Stream};
use gprimitives::{CodeId, H256};
use std::{
    collections::{HashSet, VecDeque},
    pin::Pin,
    task::{Context, Poll},
};
use tokio::task::JoinSet;

#[derive(Debug, Clone)]
enum BlockAction {
    Prepare(H256),
    Process(H256),
}

#[derive(Default)]
enum State {
    #[default]
    WaitForBlock,
    WaitForCodes {
        block: H256,
        chain: VecDeque<SimpleBlockData>,
        waiting_codes: HashSet<CodeId>,
    },
    ComputeBlock(BoxFuture<'static, Result<BlockProcessed>>),
}

// TODO #4548: add state monitoring in prometheus
// TODO #4549: add tests for compute service
pub struct ComputeService {
    db: Database,
    processor: Processor,

    blocks_queue: VecDeque<BlockAction>,
    blocks_state: State,

    process_codes: JoinSet<Result<CodeId>>,
}

impl ComputeService {
    // TODO #4550: consider to create Processor inside ComputeService
    pub fn new(db: Database, processor: Processor) -> Self {
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
                Ok(processor
                    .process_upload_code(code_and_id)
                    .map(|_valid| code_id)?)
            });
        }
    }

    pub fn prepare_block(&mut self, block: H256) {
        self.blocks_queue.push_front(BlockAction::Prepare(block));
    }

    pub fn process_block(&mut self, block: H256) {
        self.blocks_queue.push_front(BlockAction::Process(block));
    }
}

impl Stream for ComputeService {
    type Item = Result<ComputeEvent>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if let Poll::Ready(Some(res)) = self.process_codes.poll_join_next(cx) {
            match res {
                Ok(res) => {
                    if let (Ok(code_id), State::WaitForCodes { waiting_codes, .. }) =
                        (&res, &mut self.blocks_state)
                    {
                        waiting_codes.remove(code_id);
                    }

                    return Poll::Ready(Some(res.map(ComputeEvent::CodeProcessed)));
                }
                Err(e) => return Poll::Ready(Some(Err(ComputeError::CodeProcessJoin(e)))),
            }
        }

        if matches!(self.blocks_state, State::WaitForBlock) {
            match self.blocks_queue.pop_back() {
                Some(BlockAction::Prepare(block)) => {
                    let PrepareInfo {
                        chain,
                        missing_codes,
                        missing_validated_codes,
                    } = prepare::prepare(&self.db, block)?;

                    self.blocks_state = State::WaitForCodes {
                        block,
                        chain,
                        waiting_codes: missing_validated_codes,
                    };

                    return Poll::Ready(Some(Ok(ComputeEvent::RequestLoadCodes(missing_codes))));
                }
                Some(BlockAction::Process(block)) => {
                    if !self.db.block_meta(block).prepared {
                        return Poll::Ready(Some(Err(ComputeError::BlockNotPrepared(block))));
                    }

                    self.blocks_state = State::ComputeBlock(
                        compute::compute(self.db.clone(), self.processor.clone(), block).boxed(),
                    );
                }
                None => {}
            }
        }

        if let State::WaitForCodes {
            block,
            chain,
            waiting_codes,
        } = &self.blocks_state
        {
            if waiting_codes.is_empty() {
                // All codes are loaded, we can mark the block as prepared
                for block_data in chain {
                    self.db
                        .mutate_block_meta(block_data.hash, |meta| meta.prepared = true);
                }
                let event = ComputeEvent::BlockPrepared(*block);
                self.blocks_state = State::WaitForBlock;
                return Poll::Ready(Some(Ok(event)));
            }
        }

        if let State::ComputeBlock(future) = &mut self.blocks_state {
            if let Poll::Ready(res) = future.poll_unpin(cx) {
                self.blocks_state = State::WaitForBlock;
                return Poll::Ready(Some(res.map(ComputeEvent::BlockProcessed)));
            }
        }

        Poll::Pending
    }
}

impl FusedStream for ComputeService {
    fn is_terminated(&self) -> bool {
        false
    }
}
