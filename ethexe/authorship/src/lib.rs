#![allow(unused)]

use anyhow::{anyhow, Result};
use ethexe_common::gear::StateTransition;
use ethexe_db::{CodesStorage, Database};
use ethexe_observer::RequestBlockData;
use ethexe_processor::{Processor, ProcessorConfig};
use gprimitives::CodeId;
use tokio::{sync::mpsc, task::JoinHandle};

pub struct Service {
    db: Database,
    processor: Processor,
}

impl Service {
    pub fn new(config: ProcessorConfig, db: Database) -> Result<Self> {
        let processor = Processor::with_config(config, db.clone())?;
        Ok(Self { db, processor })
    }

    pub fn run(mut self) -> (JoinHandle<Result<()>>, RequestSender, EventReceiver) {
        let (request_tx, mut request_rx) = mpsc::unbounded_channel();
        let (mut event_tx, event_rx) = mpsc::unbounded_channel();

        let handle = tokio::spawn(async move {
            loop {
                tokio::select! {
                    request = request_rx.recv() => {
                        let request = request.ok_or_else(|| anyhow!("failed to receive request: channel closed"))?;

                        match request {
                            Request::HandleBlock(block) => {
                                let transitions = self.processor.process_block_events_raw(block.hash, block.events)?;

                                event_tx.send(Event::TransitionsCommitment(transitions)).map_err(|_| anyhow!("failed to send transitions commitment event"))?;
                            },
                            Request::HandleCode(code_id) => {
                                let code = self.db.original_code(code_id).ok_or_else(|| anyhow!("code not found"))?;

                                let valid = self.processor.process_upload_code_raw(code_id, &code)?;

                                event_tx.send(Event::CodeCommitment { code_id, valid }).map_err(|_| anyhow!("failed to send code commitment event"))?;
                            }
                        }
                    }
                }
            }

            Ok(())
        });

        (handle, RequestSender(request_tx), EventReceiver(event_rx))
    }
}

pub enum Event {
    CodeCommitment { code_id: CodeId, valid: bool },
    TransitionsCommitment(Vec<StateTransition>),
}

pub struct EventReceiver(mpsc::UnboundedReceiver<Event>);

impl EventReceiver {
    pub async fn recv(&mut self) -> Result<Event> {
        self.0
            .recv()
            .await
            .ok_or_else(|| anyhow!("connection closed"))
    }
}

pub enum Request {
    HandleBlock(RequestBlockData),
    HandleCode(CodeId),
}

#[derive(Clone)]
pub struct RequestSender(mpsc::UnboundedSender<Request>);

impl RequestSender {
    fn send_request(&self, request: Request) -> Result<()> {
        self.0.send(request).map_err(|_| anyhow!("service is down"))
    }

    pub fn handle_block(&self, block: RequestBlockData) -> Result<()> {
        self.send_request(Request::HandleBlock(block))
    }

    pub fn handle_code(&self, code_id: CodeId) -> Result<()> {
        self.send_request(Request::HandleCode(code_id))
    }
}
