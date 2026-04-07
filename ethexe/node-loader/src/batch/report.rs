use crate::batch::context::{Context, ContextTotals, ContextUpdate};
use std::fmt::{self, Write};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunEndedBy {
    Completed,
    Interrupted,
    Failed,
}

impl RunEndedBy {
    fn label(self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::Interrupted => "interrupted",
            Self::Failed => "failed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadRunMetadata {
    pub seed: u64,
    pub workers: usize,
    pub batch_size: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BatchExecutionStats {
    pub completed_batches: u64,
    pub failed_batches: u64,
}

impl BatchExecutionStats {
    pub fn record_completed(&mut self) {
        self.completed_batches = self.completed_batches.saturating_add(1);
    }

    pub fn record_failed(&mut self) {
        self.failed_batches = self.failed_batches.saturating_add(1);
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BatchReport {
    pub context_update: ContextUpdate,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchRunReport {
    /// Seed of the batch is the id.
    pub seed: u64,
    pub batch: BatchReport,
}

impl BatchRunReport {
    pub fn new(seed: u64, batch: BatchReport) -> Self {
        Self { seed, batch }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadRunReport {
    pub metadata: LoadRunMetadata,
    pub ended_by: RunEndedBy,
    pub context: Context,
    pub batch_stats: BatchExecutionStats,
}

impl LoadRunReport {
    pub fn totals(&self) -> ContextTotals {
        self.context.totals()
    }

    pub fn render_pretty(&self) -> String {
        let totals = self.totals();
        let stats = &totals.stats;

        let mut out = String::new();
        let _ = writeln!(out, "ethexe-node-loader summary");
        let _ = writeln!(out, "===========================");
        let _ = writeln!(out, "status: {}", self.ended_by.label());
        let _ = writeln!(out, "seed: {}", self.metadata.seed);
        let _ = writeln!(out, "workers: {}", self.metadata.workers);
        let _ = writeln!(out, "batch size: {}", self.metadata.batch_size);
        let _ = writeln!(
            out,
            "batches: {} completed, {} failed",
            self.batch_stats.completed_batches, self.batch_stats.failed_batches
        );
        let _ = writeln!(
            out,
            "programs: {} total, {} active, {} exited",
            totals.programs, totals.active_programs, totals.exited_programs
        );
        let _ = writeln!(out, "codes: {}", totals.codes);
        let _ = writeln!(out, "mailbox messages: {}", totals.mailbox_messages);
        let _ = writeln!(out, "pending value claims: {}", totals.pending_value_claims);
        let _ = writeln!(
            out,
            "message stats: {} messages, {} replies, {} state changes",
            stats.messages, stats.replies, stats.state_changes
        );
        let _ = writeln!(
            out,
            "claims: {} requested, {} succeeded, {} failed",
            stats.claims_requested, stats.claims_succeeded, stats.claims_failed
        );
        let _ = writeln!(
            out,
            "top-ups: {} executable, {} owned",
            stats.executable_topups, stats.owned_topups
        );
        let _ = write!(out, "failures observed: {}", stats.failures);

        out
    }
}

impl fmt::Display for LoadRunReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.render_pretty())
    }
}

#[cfg(test)]
mod tests {
    use super::{BatchExecutionStats, LoadRunMetadata, LoadRunReport, RunEndedBy};
    use crate::batch::context::{Context, ContextUpdate};
    use gprimitives::{ActorId, CodeId, MessageId};

    fn actor(seed: u8) -> ActorId {
        ActorId::from([seed; 32])
    }

    fn code(seed: u8) -> CodeId {
        CodeId::from([seed; 32])
    }

    fn message(seed: u8) -> MessageId {
        MessageId::from([seed; 32])
    }

    #[test]
    fn pretty_summary_includes_key_runtime_totals() {
        let actor_id = actor(1);
        let code_id = code(2);
        let mailbox_mid = message(3);

        let mut update = ContextUpdate::default();
        update.add_code(code_id);
        update.set_program_code_id(actor_id, code_id);
        update.add_mailbox_message(actor_id, mailbox_mid);
        update.stats_mut(actor_id).increment_messages();
        update.stats_mut(actor_id).increment_replies();
        update.stats_mut(actor_id).increment_failures();

        let mut context = Context::new();
        context.update(update);

        let report = LoadRunReport {
            metadata: LoadRunMetadata {
                seed: 42,
                workers: 4,
                batch_size: 8,
            },
            ended_by: RunEndedBy::Interrupted,
            context,
            batch_stats: BatchExecutionStats {
                completed_batches: 12,
                failed_batches: 2,
            },
        };

        let summary = report.render_pretty();
        assert!(summary.contains("status: interrupted"));
        assert!(summary.contains("seed: 42"));
        assert!(summary.contains("workers: 4"));
        assert!(summary.contains("batch size: 8"));
        assert!(summary.contains("batches: 12 completed, 2 failed"));
        assert!(summary.contains("programs: 1 total, 1 active, 0 exited"));
        assert!(summary.contains("codes: 1"));
        assert!(summary.contains("mailbox messages: 1"));
        assert!(summary.contains("message stats: 1 messages, 1 replies, 0 state changes"));
        assert!(summary.contains("failures observed: 1"));
    }

    #[test]
    fn pretty_summary_handles_empty_context() {
        let report = LoadRunReport {
            metadata: LoadRunMetadata {
                seed: 7,
                workers: 1,
                batch_size: 1,
            },
            ended_by: RunEndedBy::Completed,
            context: Context::new(),
            batch_stats: BatchExecutionStats::default(),
        };

        let summary = report.render_pretty();
        assert!(summary.contains("status: completed"));
        assert!(summary.contains("programs: 0 total, 0 active, 0 exited"));
        assert!(summary.contains("claims: 0 requested, 0 succeeded, 0 failed"));
    }
}
