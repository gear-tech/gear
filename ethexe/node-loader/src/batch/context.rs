use std::collections::BTreeSet;

use gprimitives::{ActorId, CodeId, MessageId};

#[derive(Debug, Default)]
pub struct ContextUpdate {
    pub program_ids: BTreeSet<ActorId>,
    pub codes: BTreeSet<CodeId>,
    pub added_mailbox: BTreeSet<MessageId>,
    pub removed_mailbox: BTreeSet<MessageId>,
    pub exited_programs: BTreeSet<ActorId>,
}

#[derive(Debug, Clone, Default)]
pub struct Context {
    pub programs: BTreeSet<ActorId>,
    pub codes: BTreeSet<CodeId>,
    pub mailbox_state: BTreeSet<MessageId>,
}

impl Context {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn update(&mut self, mut update: ContextUpdate) {
        self.programs.append(&mut update.program_ids);
        self.codes.append(&mut update.codes);
        self.mailbox_state
            .retain(|mid| !update.removed_mailbox.contains(mid));
        self.mailbox_state.append(&mut update.added_mailbox);
        // Remove exited programs from the active set
        self.programs
            .retain(|pid| !update.exited_programs.contains(pid));
    }
}
