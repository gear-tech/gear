use crate::batch::context::ContextUpdate;
use gprimitives::{ActorId, CodeId, MessageId};
use std::collections::BTreeSet;

/// Outcome of a single batch execution before it is folded into shared state.
#[derive(Default)]
pub struct Report {
    pub codes: BTreeSet<CodeId>,
    pub program_ids: BTreeSet<ActorId>,
    pub mailbox_data: MailboxReport,
    pub exited_programs: BTreeSet<ActorId>,
}

/// Mailbox mutations observed while processing a batch.
#[derive(Default)]
pub struct MailboxReport {
    pub added: BTreeSet<MessageId>,
    pub removed: BTreeSet<MessageId>,
}

impl MailboxReport {
    /// Marks mailbox messages as removed by this batch.
    #[allow(dead_code)]
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

impl From<Report> for ContextUpdate {
    fn from(v: Report) -> Self {
        ContextUpdate {
            program_ids: v.program_ids,
            codes: v.codes,
            added_mailbox: v.mailbox_data.added,
            removed_mailbox: v.mailbox_data.removed,
            exited_programs: v.exited_programs,
        }
    }
}

#[allow(dead_code)]
#[derive(Debug, Default)]
pub struct BatchRunReport {
    /// Seed used to generate the batch.
    pub id: u64,
    /// State delta derived from the batch result.
    pub context_update: ContextUpdate,
}

impl BatchRunReport {
    /// Wraps a raw [`Report`] together with the batch seed that produced it.
    pub fn new(id: u64, report: Report) -> Self {
        Self {
            id,
            context_update: report.into(),
        }
    }
}
