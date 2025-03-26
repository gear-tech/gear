use crate::utils::BatchCommitmentValidationRequest;
use ethexe_common::{ProducerBlock, SimpleBlockData};
use ethexe_signer::SignedData;

#[derive(Default)]
pub struct Initial {
    producer_blocks: Vec<SignedData<ProducerBlock>>,
    validation_requests: Vec<SignedData<BatchCommitmentValidationRequest>>,
}

impl Initial {
    pub fn receive_block_from_producer(&mut self, pb: SignedData<ProducerBlock>) {
        self.producer_blocks.push(pb);
    }

    pub fn receive_validation_request(
        &mut self,
        request: SignedData<BatchCommitmentValidationRequest>,
    ) {
        self.validation_requests.push(request);
    }

    pub fn into_unformed(self, block: SimpleBlockData) -> Unformed {
        Unformed {
            block,
            producer_blocks: self.producer_blocks,
            validation_requests: self.validation_requests,
        }
    }
}

pub struct Unformed {
    block: SimpleBlockData,
    producer_blocks: Vec<SignedData<ProducerBlock>>,
    validation_requests: Vec<SignedData<BatchCommitmentValidationRequest>>,
}

impl Unformed {
    pub fn receive_block_from_producer(&mut self, pb: SignedData<ProducerBlock>) {
        self.producer_blocks.push(pb);
    }

    pub fn receive_validation_request(
        &mut self,
        request: SignedData<BatchCommitmentValidationRequest>,
    ) {
        self.validation_requests.push(request);
    }

    pub fn into_parts(
        self,
    ) -> (
        SimpleBlockData,
        Vec<SignedData<ProducerBlock>>,
        Vec<SignedData<BatchCommitmentValidationRequest>>,
    ) {
        (self.block, self.producer_blocks, self.validation_requests)
    }

    pub fn block(&self) -> &SimpleBlockData {
        &self.block
    }
}
