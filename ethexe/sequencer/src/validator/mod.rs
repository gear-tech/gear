use anyhow::Result;
use ethexe_common::{ProducerBlock, SimpleBlockData};
use ethexe_db::Database;
use ethexe_ethereum::Ethereum;
use ethexe_observer::BlockSyncedData;
use ethexe_signer::{Address, PublicKey, SignedData, Signer};
use futures::{stream::FusedStream, Stream};
use gprimitives::H256;
use std::{
    collections::VecDeque,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

mod coordinator;
mod initial;
mod participant;
mod producer;
mod verifier;

use crate::{
    utils::{BatchCommitmentValidationReply, BatchCommitmentValidationRequest},
    ControlEvent, ControlService,
};
use initial::Initial;

pub struct ValidatorService {
    inner: Option<Box<dyn ValidatorSubService>>,
}

pub struct ValidatorConfig {
    pub ethereum_rpc: String,
    pub pub_key: PublicKey,
    pub router_address: Address,
    pub threshold: u64,
    pub slot_duration: Duration,
}

impl ValidatorService {
    pub async fn new(signer: Signer, db: Database, config: ValidatorConfig) -> Result<Self> {
        let ethereum = Ethereum::new(
            &config.ethereum_rpc,
            config.router_address,
            signer.clone(),
            config.pub_key.to_address(),
        )
        .await?;

        let ctx = ValidatorContext {
            slot_duration: config.slot_duration,
            threshold: config.threshold,
            router_address: config.router_address,
            pub_key: config.pub_key,
            signer,
            db,
            ethereum,
            pending_events: VecDeque::new(),
            output: VecDeque::new(),
        };

        Ok(Self {
            inner: Some(Initial::create(ctx)?),
        })
    }

    fn context(&self) -> &ValidatorContext {
        self.inner
            .as_ref()
            .unwrap_or_else(|| unreachable!("inner must be Some"))
            .context()
    }

    fn take_inner(&mut self) -> Box<dyn ValidatorSubService> {
        self.inner
            .take()
            .unwrap_or_else(|| unreachable!("inner must be Some"))
    }
}

impl ControlService for ValidatorService {
    fn role(&self) -> String {
        format!("Validator ({:?})", self.context().pub_key.to_address())
    }

    // TODO #4555: block producer could be calculated right here, using propagation from previous blocks.
    fn receive_new_chain_head(&mut self, block: SimpleBlockData) -> Result<()> {
        self.take_inner().process_new_head(block).map(|inner| {
            self.inner = Some(inner);
        })
    }

    fn receive_synced_block(&mut self, data: BlockSyncedData) -> Result<()> {
        self.take_inner().process_synced_block(data).map(|inner| {
            self.inner = Some(inner);
        })
    }

    fn receive_block_from_producer(&mut self, signed: SignedData<ProducerBlock>) -> Result<()> {
        self.take_inner()
            .process_input_event(InputEvent::ProducerBlock(signed))
            .map(|inner| {
                self.inner = Some(inner);
            })
    }

    fn receive_computed_block(&mut self, computed_block: H256) -> Result<()> {
        self.take_inner()
            .process_computed_block(computed_block)
            .map(|inner| {
                self.inner = Some(inner);
            })
    }

    fn receive_validation_request(
        &mut self,
        signed_request: SignedData<BatchCommitmentValidationRequest>,
    ) -> Result<()> {
        self.take_inner()
            .process_input_event(InputEvent::ValidationRequest(signed_request))
            .map(|inner| {
                self.inner = Some(inner);
            })
    }

    fn receive_validation_reply(&mut self, reply: BatchCommitmentValidationReply) -> Result<()> {
        self.take_inner()
            .process_input_event(InputEvent::ValidationReply(reply))
            .map(|inner| {
                self.inner = Some(inner);
            })
    }

    fn is_block_producer(&self) -> Result<bool> {
        // +_+_+
        todo!()
    }
}

impl Stream for ValidatorService {
    type Item = Result<ControlEvent>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.inner = Some(self.take_inner().poll(cx)?);

        self.take_inner()
            .context_mut()
            .output
            .pop_front()
            .map(|event| Poll::Ready(Some(Ok(event))))
            .unwrap_or(Poll::Pending)
    }
}

impl FusedStream for ValidatorService {
    fn is_terminated(&self) -> bool {
        false
    }
}

#[derive(Debug)]
enum InputEvent {
    ProducerBlock(SignedData<ProducerBlock>),
    ValidationRequest(SignedData<BatchCommitmentValidationRequest>),
    ValidationReply(BatchCommitmentValidationReply),
}

trait ValidatorSubService: Unpin + Send + 'static {
    fn to_dyn(self: Box<Self>) -> Box<dyn ValidatorSubService>;
    fn context(&self) -> &ValidatorContext;
    fn context_mut(&mut self) -> &mut ValidatorContext;
    fn into_context(self: Box<Self>) -> ValidatorContext;

    fn process_input_event(
        mut self: Box<Self>,
        event: InputEvent,
    ) -> Result<Box<dyn ValidatorSubService>> {
        self.context_mut().warning(format!(
            "Unexpected input event: {event:?}, append to pending events"
        ));

        self.context_mut().pending_events.push_back(event);

        Ok(self.to_dyn())
    }

    fn process_new_head(
        self: Box<Self>,
        block: SimpleBlockData,
    ) -> Result<Box<dyn ValidatorSubService>> {
        Initial::create_with_chain_head(self.into_context(), block)
    }

    fn process_synced_block(
        mut self: Box<Self>,
        data: BlockSyncedData,
    ) -> Result<Box<dyn ValidatorSubService>> {
        self.context_mut()
            .warning(format!("Unexpected synced block: {:?}", data.block_hash));

        Ok(self.to_dyn())
    }

    fn process_computed_block(
        mut self: Box<Self>,
        computed_block: H256,
    ) -> Result<Box<dyn ValidatorSubService>> {
        self.context_mut()
            .warning(format!("Unexpected computed block: {computed_block:?}"));

        Ok(self.to_dyn())
    }

    fn poll(self: Box<Self>, _cx: &mut Context<'_>) -> Result<Box<dyn ValidatorSubService>> {
        Ok(self.to_dyn())
    }
}

struct ValidatorContext {
    slot_duration: Duration,
    threshold: u64,
    router_address: Address,
    pub_key: PublicKey,
    signer: Signer,
    db: Database,
    ethereum: Ethereum,
    pending_events: VecDeque<InputEvent>,
    output: VecDeque<ControlEvent>,
}

impl ValidatorContext {
    pub fn warning(&mut self, warning: String) {
        self.output.push_back(ControlEvent::Warning(warning));
    }
}
