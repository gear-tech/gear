use super::context::ContextUpdate;
use crate::utils;
use anyhow::Error;
use gear_core::ids::{CodeId, MessageId, ProgramId};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, thiserror::Error)]
pub enum CrashAlert {
    #[error("Crash alert: Message processing has been stopped")]
    MsgProcessingStopped,
    #[error("Crash alert: Timeout occurred while waiting for events")]
    EventsTimeout,
    #[error("Crash alert: Timeout occurred while waiting for transaction to be finalized.")]
    TransactionTimeout,
    #[error("Crash alert: Can't reach the node, considered to be dead")]
    NodeIsDead,
}

impl TryFrom<Error> for CrashAlert {
    type Error = Error;

    fn try_from(err: Error) -> Result<Self, Self::Error> {
        let err_string = err.to_string().to_lowercase();
        if err_string.contains(&utils::EVENTS_TIMEOUT_ERR_STR.to_lowercase()) {
            Ok(CrashAlert::EventsTimeout)
        } else if err_string.contains(&utils::WAITING_TX_FINALIZED_TIMEOUT_ERR_STR.to_lowercase()) {
            Ok(CrashAlert::TransactionTimeout)
        } else if err_string.contains(&utils::SUBXT_RPC_REQUEST_ERR_STR.to_lowercase()) {
            Ok(CrashAlert::NodeIsDead)
        } else {
            Err(err)
        }
    }
}

#[derive(Default)]
pub struct Report {
    pub codes: BTreeSet<CodeId>,
    pub program_ids: BTreeSet<ProgramId>,
    pub mailbox_data: MailboxReport,
}

#[derive(Default)]
pub struct MailboxReport {
    pub added: BTreeSet<MessageId>,
    pub removed: BTreeSet<MessageId>,
}

impl MailboxReport {
    pub fn append_removed(&mut self, removed: impl IntoIterator<Item = MessageId>) {
        self.removed.append(&mut BTreeSet::from_iter(removed));
    }
}

impl From<BTreeSet<MessageId>> for MailboxReport {
    fn from(v: BTreeSet<MessageId>) -> Self {
        MailboxReport {
            added: v,
            removed: Default::default(),
        }
    }
}

#[derive(Debug, Default)]
pub struct BatchRunReport {
    /// Seed of the batch is the id.
    pub id: u64,
    pub context_update: ContextUpdate,
}

impl BatchRunReport {
    pub fn new(id: u64, report: Report) -> Self {
        Self {
            id,
            context_update: report.into(),
        }
    }

    pub fn empty(id: u64) -> Self {
        Self {
            id,
            ..Default::default()
        }
    }
}
