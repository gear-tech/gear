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
mod submitter;
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

    fn update_inner(
        &mut self,
        update: impl FnOnce(Box<dyn ValidatorSubService>) -> Result<Box<dyn ValidatorSubService>>,
    ) -> Result<()> {
        let inner = self
            .inner
            .take()
            .unwrap_or_else(|| unreachable!("inner must be Some"));

        update(inner).map(|inner| {
            self.inner = Some(inner);
        })
    }
}

impl ControlService for ValidatorService {
    fn role(&self) -> String {
        format!("Validator ({:?})", self.context().pub_key.to_address())
    }

    fn receive_new_chain_head(&mut self, block: SimpleBlockData) -> Result<()> {
        self.update_inner(|inner| inner.process_new_head(block))
    }

    fn receive_synced_block(&mut self, data: BlockSyncedData) -> Result<()> {
        self.update_inner(|inner| inner.process_synced_block(data))
    }

    fn receive_block_from_producer(&mut self, signed: SignedData<ProducerBlock>) -> Result<()> {
        self.update_inner(|inner| {
            inner.process_external_event(ExternalEvent::ProducerBlock(signed))
        })
    }

    fn receive_computed_block(&mut self, computed_block: H256) -> Result<()> {
        self.update_inner(|inner| inner.process_computed_block(computed_block))
    }

    fn receive_validation_request(
        &mut self,
        signed_request: SignedData<BatchCommitmentValidationRequest>,
    ) -> Result<()> {
        self.update_inner(|inner| {
            inner.process_external_event(ExternalEvent::ValidationRequest(signed_request))
        })
    }

    fn receive_validation_reply(&mut self, reply: BatchCommitmentValidationReply) -> Result<()> {
        self.update_inner(|inner| {
            inner.process_external_event(ExternalEvent::ValidationReply(reply))
        })
    }
}

impl Stream for ValidatorService {
    type Item = Result<ControlEvent>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut event = None;
        self.update_inner(|inner| {
            let mut inner = inner.poll(cx)?;

            event = inner.context_mut().output.pop_front();

            Ok(inner)
        })?;

        event
            .map(|event| Poll::Ready(Some(Ok(event))))
            .unwrap_or(Poll::Pending)
    }
}

impl FusedStream for ValidatorService {
    fn is_terminated(&self) -> bool {
        false
    }
}

#[derive(Debug, derive_more::From)]
enum ExternalEvent {
    ProducerBlock(SignedData<ProducerBlock>),
    ValidationRequest(SignedData<BatchCommitmentValidationRequest>),
    ValidationReply(BatchCommitmentValidationReply),
}

trait ValidatorSubService: Unpin + Send + 'static {
    fn log(&self, s: String) -> String;
    fn to_dyn(self: Box<Self>) -> Box<dyn ValidatorSubService>;
    fn context(&self) -> &ValidatorContext;
    fn context_mut(&mut self) -> &mut ValidatorContext;
    fn into_context(self: Box<Self>) -> ValidatorContext;

    fn process_external_event(
        mut self: Box<Self>,
        event: ExternalEvent,
    ) -> Result<Box<dyn ValidatorSubService>> {
        if matches!(event, ExternalEvent::ValidationReply(_)) {
            log::trace!(
                "Skip validation reply: {event:?}, because only coordinator should process it"
            );

            return Ok(self.to_dyn());
        }

        let warning = self.log(format!("unexpected event: {event:?}, save for later"));
        self.context_mut().warning(warning);

        self.context_mut().pending_events.push_back(event);

        Ok(self.to_dyn())
    }

    fn process_new_head(
        self: Box<Self>,
        block: SimpleBlockData,
    ) -> Result<Box<dyn ValidatorSubService>> {
        // TODO #4555: block producer could be calculated right here, using propagation from previous blocks.
        Initial::create_with_chain_head(self.into_context(), block)
    }

    fn process_synced_block(
        mut self: Box<Self>,
        data: BlockSyncedData,
    ) -> Result<Box<dyn ValidatorSubService>> {
        let warning = self.log(format!("Unexpected synced block: {:?}", data.block_hash));
        self.context_mut().warning(warning);

        Ok(self.to_dyn())
    }

    fn process_computed_block(
        mut self: Box<Self>,
        computed_block: H256,
    ) -> Result<Box<dyn ValidatorSubService>> {
        let warning = self.log(format!("Unexpected computed block: {computed_block:?}"));
        self.context_mut().warning(warning);

        Ok(self.to_dyn())
    }

    fn poll(self: Box<Self>, _cx: &mut Context<'_>) -> Result<Box<dyn ValidatorSubService>> {
        Ok(self.to_dyn())
    }

    fn warning(&mut self, warning: String) {
        let warning = self.log(warning);
        self.context_mut().warning(warning);
    }

    fn output(&mut self, event: ControlEvent) {
        self.context_mut().output(event);
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
    pending_events: VecDeque<ExternalEvent>,
    output: VecDeque<ControlEvent>,
}

impl ValidatorContext {
    pub fn warning(&mut self, warning: String) {
        self.output.push_back(ControlEvent::Warning(warning));
    }

    pub fn output(&mut self, event: ControlEvent) {
        self.output.push_back(event);
    }
}
