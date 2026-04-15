use gprimitives::{ActorId, CodeId, H256, MessageId};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProgramStats {
    pub messages: u64,
    pub replies: u64,
    pub mailbox_additions: u64,
    pub claims_requested: u64,
    pub claims_succeeded: u64,
    pub claims_failed: u64,
    pub state_changes: u64,
    pub executable_topups: u64,
    pub owned_topups: u64,
    pub failures: u64,
}

impl ProgramStats {
    pub fn increment_messages(&mut self) {
        self.messages = self.messages.saturating_add(1);
    }

    pub fn increment_replies(&mut self) {
        self.replies = self.replies.saturating_add(1);
    }

    pub fn increment_mailbox_additions(&mut self) {
        self.mailbox_additions = self.mailbox_additions.saturating_add(1);
    }

    pub fn increment_claims_requested(&mut self) {
        self.claims_requested = self.claims_requested.saturating_add(1);
    }

    pub fn increment_claims_succeeded(&mut self) {
        self.claims_succeeded = self.claims_succeeded.saturating_add(1);
    }

    pub fn increment_claims_failed(&mut self) {
        self.claims_failed = self.claims_failed.saturating_add(1);
    }

    pub fn increment_state_changes(&mut self) {
        self.state_changes = self.state_changes.saturating_add(1);
    }

    pub fn increment_executable_topups(&mut self) {
        self.executable_topups = self.executable_topups.saturating_add(1);
    }

    pub fn increment_owned_topups(&mut self) {
        self.owned_topups = self.owned_topups.saturating_add(1);
    }

    pub fn increment_failures(&mut self) {
        self.failures = self.failures.saturating_add(1);
    }

    fn accumulate(&mut self, rhs: &Self) {
        self.messages = self.messages.saturating_add(rhs.messages);
        self.replies = self.replies.saturating_add(rhs.replies);
        self.mailbox_additions = self.mailbox_additions.saturating_add(rhs.mailbox_additions);
        self.claims_requested = self.claims_requested.saturating_add(rhs.claims_requested);
        self.claims_succeeded = self.claims_succeeded.saturating_add(rhs.claims_succeeded);
        self.claims_failed = self.claims_failed.saturating_add(rhs.claims_failed);
        self.state_changes = self.state_changes.saturating_add(rhs.state_changes);
        self.executable_topups = self.executable_topups.saturating_add(rhs.executable_topups);
        self.owned_topups = self.owned_topups.saturating_add(rhs.owned_topups);
        self.failures = self.failures.saturating_add(rhs.failures);
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ContextTotals {
    pub programs: usize,
    pub active_programs: usize,
    pub exited_programs: usize,
    pub codes: usize,
    pub mailbox_messages: usize,
    pub pending_value_claims: usize,
    pub stats: ProgramStats,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProgramContext {
    pub code_id: Option<CodeId>,
    pub exited: bool,
    pub last_state_hash: Option<H256>,
    pub mailbox: BTreeSet<MessageId>,
    pub pending_value_claims: BTreeSet<MessageId>,
    pub stats: ProgramStats,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProgramContextUpdate {
    pub code_id: Option<CodeId>,
    pub exited: Option<bool>,
    pub last_state_hash: Option<H256>,
    pub mailbox_added: BTreeSet<MessageId>,
    pub mailbox_removed: BTreeSet<MessageId>,
    pub pending_value_claims_added: BTreeSet<MessageId>,
    pub pending_value_claims_removed: BTreeSet<MessageId>,
    pub stats_delta: ProgramStats,
}

/// Delta produced by a batch execution and applied to the shared generator state.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ContextUpdate {
    pub codes: BTreeSet<CodeId>,
    pub message_owners: BTreeMap<MessageId, ActorId>,
    pub programs: BTreeMap<ActorId, ProgramContextUpdate>,
}

impl ContextUpdate {
    pub fn upsert_message_owner(&mut self, message_id: MessageId, actor_id: ActorId) {
        self.message_owners.insert(message_id, actor_id);
    }

    pub fn add_code(&mut self, code_id: CodeId) {
        self.codes.insert(code_id);
    }

    pub fn set_program_code_id(&mut self, actor_id: ActorId, code_id: CodeId) {
        self.program_update(actor_id).code_id = Some(code_id);
    }

    pub fn set_program_exited(&mut self, actor_id: ActorId, exited: bool) {
        self.program_update(actor_id).exited = Some(exited);
    }

    pub fn set_program_last_state_hash(&mut self, actor_id: ActorId, state_hash: H256) {
        self.program_update(actor_id).last_state_hash = Some(state_hash);
    }

    pub fn add_mailbox_message(&mut self, actor_id: ActorId, message_id: MessageId) {
        let update = self.program_update(actor_id);
        update.mailbox_added.insert(message_id);
        update.mailbox_removed.remove(&message_id);
    }

    pub fn remove_mailbox_message(&mut self, actor_id: ActorId, message_id: MessageId) {
        let update = self.program_update(actor_id);
        update.mailbox_removed.insert(message_id);
        update.mailbox_added.remove(&message_id);
    }

    pub fn add_pending_value_claim(&mut self, actor_id: ActorId, message_id: MessageId) {
        let update = self.program_update(actor_id);
        update.pending_value_claims_added.insert(message_id);
        update.pending_value_claims_removed.remove(&message_id);
    }

    pub fn remove_pending_value_claim(&mut self, actor_id: ActorId, message_id: MessageId) {
        let update = self.program_update(actor_id);
        update.pending_value_claims_removed.insert(message_id);
        update.pending_value_claims_added.remove(&message_id);
    }

    pub fn stats_mut(&mut self, actor_id: ActorId) -> &mut ProgramStats {
        &mut self.program_update(actor_id).stats_delta
    }

    fn program_update(&mut self, actor_id: ActorId) -> &mut ProgramContextUpdate {
        self.programs.entry(actor_id).or_default()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Context {
    pub programs: BTreeMap<ActorId, ProgramContext>,
    pub codes: BTreeSet<CodeId>,
    pub message_owners: BTreeMap<MessageId, ActorId>,
}

impl Context {
    /// Creates an empty generation context.
    pub fn new() -> Self {
        Default::default()
    }

    /// Applies one batch result to the shared context.
    pub fn update(&mut self, update: ContextUpdate) {
        let ContextUpdate {
            mut codes,
            message_owners,
            programs,
        } = update;

        self.codes.append(&mut codes);
        self.message_owners.extend(message_owners);

        for (actor_id, update) in programs {
            let program = self.programs.entry(actor_id).or_default();

            if let Some(code_id) = update.code_id {
                program.code_id = Some(code_id);
            }
            if let Some(exited) = update.exited {
                program.exited = exited;
            }
            if let Some(state_hash) = update.last_state_hash {
                program.last_state_hash = Some(state_hash);
            }

            for mid in update.mailbox_removed {
                program.mailbox.remove(&mid);
            }
            program.mailbox.extend(update.mailbox_added);

            for mid in update.pending_value_claims_removed {
                program.pending_value_claims.remove(&mid);
            }
            program
                .pending_value_claims
                .extend(update.pending_value_claims_added);

            program.stats.accumulate(&update.stats_delta);
        }
    }

    pub fn active_program_ids(&self) -> Vec<ActorId> {
        self.programs
            .iter()
            .filter_map(|(&actor_id, program)| (!program.exited).then_some(actor_id))
            .collect()
    }

    pub fn all_program_ids(&self) -> Vec<ActorId> {
        self.programs.keys().copied().collect()
    }

    pub fn all_mailbox_message_ids(&self) -> Vec<MessageId> {
        self.programs
            .values()
            .flat_map(|program| program.mailbox.iter().copied())
            .collect()
    }

    pub fn all_code_ids(&self) -> Vec<CodeId> {
        self.codes.iter().copied().collect()
    }

    pub fn owner_of(&self, message_id: MessageId) -> Option<ActorId> {
        self.message_owners.get(&message_id).copied()
    }

    pub fn totals(&self) -> ContextTotals {
        let mut totals = ContextTotals {
            programs: self.programs.len(),
            active_programs: self
                .programs
                .values()
                .filter(|program| !program.exited)
                .count(),
            exited_programs: self
                .programs
                .values()
                .filter(|program| program.exited)
                .count(),
            codes: self.codes.len(),
            mailbox_messages: self
                .programs
                .values()
                .map(|program| program.mailbox.len())
                .sum(),
            pending_value_claims: self
                .programs
                .values()
                .map(|program| program.pending_value_claims.len())
                .sum(),
            stats: ProgramStats::default(),
        };

        for program in self.programs.values() {
            totals.stats.accumulate(&program.stats);
        }

        totals
    }
}

#[cfg(test)]
mod tests {
    use super::{Context, ContextUpdate};
    use gprimitives::{ActorId, CodeId, H256, MessageId};

    fn actor(seed: u8) -> ActorId {
        ActorId::from([seed; 32])
    }

    fn code(seed: u8) -> CodeId {
        CodeId::from([seed; 32])
    }

    fn message(seed: u8) -> MessageId {
        MessageId::from([seed; 32])
    }

    fn hash(seed: u8) -> H256 {
        H256::from([seed; 32])
    }

    #[test]
    fn context_update_tracks_program_state_without_deleting_exited_programs() {
        let actor_id = actor(1);
        let code_id = code(2);
        let mailbox_mid = message(3);
        let claim_mid = message(4);
        let next_mid = message(5);
        let state_hash = hash(6);

        let mut update = ContextUpdate::default();
        update.add_code(code_id);
        update.set_program_code_id(actor_id, code_id);
        update.upsert_message_owner(mailbox_mid, actor_id);
        update.upsert_message_owner(next_mid, actor_id);
        update.add_mailbox_message(actor_id, mailbox_mid);
        update.add_pending_value_claim(actor_id, claim_mid);
        update.set_program_last_state_hash(actor_id, state_hash);
        update.stats_mut(actor_id).increment_messages();
        update.stats_mut(actor_id).increment_mailbox_additions();
        update.stats_mut(actor_id).increment_claims_requested();
        update.stats_mut(actor_id).increment_state_changes();

        let mut context = Context::new();
        context.update(update);

        let program = context.programs.get(&actor_id).expect("program registered");
        assert_eq!(program.code_id, Some(code_id));
        assert_eq!(program.last_state_hash, Some(state_hash));
        assert!(program.mailbox.contains(&mailbox_mid));
        assert!(program.pending_value_claims.contains(&claim_mid));
        assert_eq!(program.stats.messages, 1);
        assert_eq!(program.stats.mailbox_additions, 1);
        assert_eq!(program.stats.claims_requested, 1);
        assert_eq!(program.stats.state_changes, 1);
        assert_eq!(context.owner_of(next_mid), Some(actor_id));

        let mut second = ContextUpdate::default();
        second.remove_mailbox_message(actor_id, mailbox_mid);
        second.remove_pending_value_claim(actor_id, claim_mid);
        second.set_program_exited(actor_id, true);
        second.stats_mut(actor_id).increment_claims_succeeded();
        second.stats_mut(actor_id).increment_failures();
        context.update(second);

        let program = context.programs.get(&actor_id).expect("program retained");
        assert!(program.exited);
        assert!(program.mailbox.is_empty());
        assert!(program.pending_value_claims.is_empty());
        assert_eq!(program.stats.claims_succeeded, 1);
        assert_eq!(program.stats.failures, 1);
        assert!(context.active_program_ids().is_empty());
        assert_eq!(context.all_program_ids(), vec![actor_id]);
        assert_eq!(context.all_code_ids(), vec![code_id]);
    }

    #[test]
    fn context_collects_mailbox_message_ids_from_all_programs() {
        let actor_a = actor(10);
        let actor_b = actor(11);
        let mid_a = message(12);
        let mid_b = message(13);

        let mut update = ContextUpdate::default();
        update.add_mailbox_message(actor_a, mid_a);
        update.add_mailbox_message(actor_b, mid_b);

        let mut context = Context::new();
        context.update(update);

        let mut mailbox = context.all_mailbox_message_ids();
        mailbox.sort();
        assert_eq!(mailbox, vec![mid_a, mid_b]);
    }

    #[test]
    fn context_totals_roll_up_program_stats_and_counts() {
        let actor_a = actor(20);
        let actor_b = actor(21);
        let code_a = code(22);
        let code_b = code(23);
        let mailbox_mid = message(24);
        let claim_mid = message(25);

        let mut first = ContextUpdate::default();
        first.add_code(code_a);
        first.add_code(code_b);
        first.set_program_code_id(actor_a, code_a);
        first.set_program_code_id(actor_b, code_b);
        first.add_mailbox_message(actor_a, mailbox_mid);
        first.add_pending_value_claim(actor_b, claim_mid);
        first.stats_mut(actor_a).increment_messages();
        first.stats_mut(actor_a).increment_replies();
        first.stats_mut(actor_b).increment_failures();
        first.set_program_exited(actor_b, true);

        let mut context = Context::new();
        context.update(first);

        let totals = context.totals();
        assert_eq!(totals.programs, 2);
        assert_eq!(totals.active_programs, 1);
        assert_eq!(totals.exited_programs, 1);
        assert_eq!(totals.codes, 2);
        assert_eq!(totals.mailbox_messages, 1);
        assert_eq!(totals.pending_value_claims, 1);
        assert_eq!(totals.stats.messages, 1);
        assert_eq!(totals.stats.replies, 1);
        assert_eq!(totals.stats.failures, 1);
    }
}
