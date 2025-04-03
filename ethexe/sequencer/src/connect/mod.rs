use crate::{
    utils::{BatchCommitmentValidationReply, BatchCommitmentValidationRequest},
    ControlEvent, ControlService,
};
use anyhow::Result;
use ethexe_common::{ProducerBlock, SimpleBlockData};
use ethexe_observer::BlockSyncedData;
use ethexe_signer::SignedData;
use futures::{stream::FusedStream, Stream};
use gprimitives::H256;
use std::{
    collections::VecDeque,
    pin::Pin,
    task::{Context, Poll},
};

#[derive(Debug, Default)]
pub struct SimpleConnectService {
    block: Option<SimpleBlockData>,
    output: VecDeque<ControlEvent>,
}

impl SimpleConnectService {
    pub fn new() -> Self {
        Self::default()
    }
}

impl ControlService for SimpleConnectService {
    fn role(&self) -> String {
        "Connect".to_string()
    }

    fn receive_new_chain_head(&mut self, block: SimpleBlockData) -> Result<()> {
        self.block = Some(block);

        Ok(())
    }

    fn receive_synced_block(&mut self, data: BlockSyncedData) -> Result<()> {
        let Some(block) = self.block.as_ref() else {
            self.output.push_back(ControlEvent::Warning(format!(
                "Received synced block {}, but no chain-head was received yet",
                data.block_hash
            )));

            return Ok(());
        };

        if block.hash != data.block_hash {
            self.output.push_back(ControlEvent::Warning(format!(
                "Received synced block {} is different from the expected block hash {}",
                data.block_hash, block.hash
            )));

            return Ok(());
        }

        self.output
            .push_back(ControlEvent::ComputeBlock(block.header.parent_hash));

        Ok(())
    }

    fn receive_block_from_producer(
        &mut self,
        _block_hash: SignedData<ProducerBlock>,
    ) -> Result<()> {
        Ok(())
    }

    fn receive_computed_block(&mut self, _block_hash: H256) -> Result<()> {
        Ok(())
    }

    fn receive_validation_request(
        &mut self,
        _signed_batch: SignedData<BatchCommitmentValidationRequest>,
    ) -> Result<()> {
        Ok(())
    }

    fn receive_validation_reply(&mut self, _reply: BatchCommitmentValidationReply) -> Result<()> {
        Ok(())
    }

    fn is_block_producer(&self) -> Result<bool> {
        Ok(false)
    }
}

impl Stream for SimpleConnectService {
    type Item = anyhow::Result<ControlEvent>;

    fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if let Some(event) = self.output.pop_front() {
            Poll::Ready(Some(Ok(event)))
        } else {
            Poll::Pending
        }
    }
}

impl FusedStream for SimpleConnectService {
    fn is_terminated(&self) -> bool {
        false
    }
}
