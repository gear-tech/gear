use std::collections::BTreeSet;

use gprimitives::{ActorId, CodeId, MessageId};

use crate::batch::context::ContextUpdate;

#[derive(Default)]
pub struct Report {
    pub codes: BTreeSet<CodeId>,
    pub program_ids: BTreeSet<ActorId>,
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

impl From<Report> for ContextUpdate {
    fn from(v: Report) -> Self {
        ContextUpdate {
            program_ids: v.program_ids,
            codes: v.codes,
            added_mailbox: v.mailbox_data.added,
            removed_mailbox: v.mailbox_data.removed,
        }
    }
}

#[allow(dead_code)]
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
