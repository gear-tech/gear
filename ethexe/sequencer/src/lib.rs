// TODO +_+_+: doc

mod connect;
mod utils;
mod validator;

pub use connect::SimpleConnectService;
pub use utils::{BatchCommitmentValidationReply, BatchCommitmentValidationRequest};
pub use validator::{ValidatorConfig, ValidatorService};

use anyhow::Result;
use ethexe_common::{ProducerBlock, SimpleBlockData};
use ethexe_observer::BlockSyncedData;
use ethexe_signer::{Address, SignedData};
use futures::{stream::FusedStream, Stream};
use gprimitives::H256;

pub trait ControlService:
    Stream<Item = anyhow::Result<ControlEvent>> + FusedStream + Unpin + Send + 'static
{
    fn role(&self) -> String;
    fn receive_new_chain_head(&mut self, block: SimpleBlockData) -> Result<()>;
    fn receive_synced_block(&mut self, data: BlockSyncedData) -> Result<()>;
    fn receive_block_from_producer(&mut self, block: SignedData<ProducerBlock>) -> Result<()>;
    fn receive_computed_block(&mut self, block_hash: H256) -> Result<()>;
    fn receive_validation_request(
        &mut self,
        request: SignedData<BatchCommitmentValidationRequest>,
    ) -> Result<()>;
    fn receive_validation_reply(&mut self, reply: BatchCommitmentValidationReply) -> Result<()>;
    fn is_block_producer(&self) -> Result<bool>;
}

#[derive(Debug, Clone)]
pub enum ControlEvent {
    IAmProducer(Address),
    IAmSubordinate {
        my_address: Address,
        producer: Address,
    },
    ComputeBlock(H256),
    ComputeProducerBlock(ProducerBlock),
    PublishProducerBlock(SignedData<ProducerBlock>),
    PublishValidationRequest(SignedData<BatchCommitmentValidationRequest>),
    PublishValidationReply(BatchCommitmentValidationReply),
    CommitmentSubmitted(H256),
    Warning(String),
}

#[cfg(test)]
mod test_utils {
    use crate::BatchCommitmentValidationRequest;
    use ethexe_common::{
        gear::{CodeCommitment, Message, StateTransition},
        ProducerBlock, SimpleBlockData,
    };
    use ethexe_db::BlockHeader;
    use ethexe_signer::{PrivateKey, PublicKey, SignedData, Signer};
    use gprimitives::H256;

    pub fn init_signer_with_keys(amount: u8) -> (Signer, Vec<PrivateKey>, Vec<PublicKey>) {
        let signer = Signer::tmp();
        let private_keys: Vec<_> = (0..amount).map(|i| PrivateKey([i + 1; 32])).collect();
        let public_keys = private_keys
            .iter()
            .map(|&key| signer.add_key(key).unwrap())
            .collect();
        (signer, private_keys, public_keys)
    }

    pub fn mock_simple_block_data() -> SimpleBlockData {
        let block_hash = H256::random();
        let parent_hash = H256::random();
        SimpleBlockData {
            hash: block_hash,
            header: BlockHeader {
                height: 43,
                timestamp: 120,
                parent_hash,
            },
        }
    }

    pub fn mock_producer_block(
        signer: &Signer,
        producer: PublicKey,
        block_hash: H256,
    ) -> (ProducerBlock, SignedData<ProducerBlock>) {
        let pb = ProducerBlock {
            block_hash,
            gas_allowance: Some(100),
            off_chain_transactions: vec![],
        };

        let signed_pb = signer.create_signed_data(producer, pb.clone()).unwrap();

        (pb, signed_pb)
    }

    pub fn mock_validation_request(
        signer: &Signer,
        public_key: PublicKey,
    ) -> (
        BatchCommitmentValidationRequest,
        SignedData<BatchCommitmentValidationRequest>,
    ) {
        let request = BatchCommitmentValidationRequest {
            blocks: vec![],
            codes: vec![],
        };
        let signed = signer
            .create_signed_data(public_key, request.clone())
            .unwrap();
        (request, signed)
    }

    pub fn mock_code_commitment() -> CodeCommitment {
        CodeCommitment {
            id: H256::random().into(),
            timestamp: 123,
            valid: true,
        }
    }

    pub fn mock_state_transition() -> StateTransition {
        StateTransition {
            actor_id: H256::random().into(),
            new_state_hash: H256::random(),
            inheritor: H256::random().into(),
            value_to_receive: 123,
            value_claims: vec![],
            messages: vec![Message {
                id: H256::random().into(),
                destination: H256::random().into(),
                payload: b"Hello, World!".to_vec(),
                value: 0,
                reply_details: None,
            }],
        }
    }
}
