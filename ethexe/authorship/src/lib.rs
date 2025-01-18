#![allow(unused)]

use anyhow::Result;
use ethexe_processor::Processor;
use gprimitives::CodeId;
use tokio::sync::mpsc;

pub struct Service {
    sender: mpsc::UnboundedSender<Event>,
    receiver: mpsc::UnboundedReceiver<Request>,

    processor: Processor,
}

impl Service {
    // creates production version of self
    pub fn new(config: &Config) -> Result<Self> {
        todo!("PR");
    }

    /// spawns `|| loop { select! { .. } }` in separate tokio thread
    /// returns external IO channels
    pub fn run(
        self,
    ) -> (
        mpsc::UnboundedSender<Request>,
        mpsc::UnboundedReceiver<Event>,
    ) {
        todo!("PR");
    }
}

pub struct Config {/* configuration */}

pub enum Event {
    BlockCommitment {/* commitment params like transitions and block id */},
    CodeCommitment { code_id: CodeId, valid: bool },
}

pub enum Request {
    HandleBlock(/* block data */),
    HandleCode(CodeId),
}
