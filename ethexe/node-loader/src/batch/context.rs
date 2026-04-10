use gprimitives::{ActorId, CodeId, MessageId};
use std::collections::BTreeSet;

/// Delta produced by a batch execution and applied to the shared generator state.
#[derive(Debug, Default)]
pub struct ContextUpdate {
    pub program_ids: BTreeSet<ActorId>,
    pub codes: BTreeSet<CodeId>,
    pub added_mailbox: BTreeSet<MessageId>,
    pub removed_mailbox: BTreeSet<MessageId>,
    pub exited_programs: BTreeSet<ActorId>,
}

/// Minimal mutable state needed to generate valid follow-up batches.
///
/// The generator uses this snapshot to decide whether it can create
/// `send_message`, `send_reply`, `claim_value`, or `create_program` batches.
#[derive(Debug, Clone, Default)]
pub struct Context {
    pub programs: BTreeSet<ActorId>,
    pub codes: BTreeSet<CodeId>,
    pub mailbox_state: BTreeSet<MessageId>,
}

impl Context {
    /// Creates an empty generation context.
    pub fn new() -> Self {
        Default::default()
    }

    /// Applies one batch result to the shared context.
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
