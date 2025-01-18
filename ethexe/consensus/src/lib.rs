#![allow(unused)]

use anyhow::Result;
use ethexe_signer::Signer;
use gprimitives::CodeId;
use tokio::sync::mpsc;

pub struct Service {
    sender: mpsc::UnboundedSender<Event>,
    receiver: mpsc::UnboundedReceiver<Request>,

    // TODO: handle if is validator
    signer: Option<Signer>,
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
    BlockCommitmentVerified {/* commitment params like transitions and block id */},
    CodeCommitmentVerified {/* applicable arguments */},
}

pub enum Request {
    VerifyBlockCommitment(/* block data */),
    VerifyCodeCommitment(/* applicable data */),
}
