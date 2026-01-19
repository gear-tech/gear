use std::collections::BTreeSet;

use gprimitives::{ActorId, CodeId, MessageId};

#[derive(Debug, Default)]
pub struct ContextUpdate {
    program_ids: BTreeSet<ActorId>,
    codes: BTreeSet<CodeId>,
    added_mailbox: BTreeSet<MessageId>,
    removed_mailbox: BTreeSet<MessageId>,
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
    }
}
