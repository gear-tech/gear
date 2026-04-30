use crate::batch::{
    context::{Context, ContextTotals, ContextUpdate},
    value::{BudgetExhaustion, ValuePolicy, format_wei, format_wvara},
};
use std::fmt::{self, Write};

/// Final status of one load-generator run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunEndedBy {
    Completed,
    Interrupted,
    Failed,
    BudgetExhausted,
}

impl RunEndedBy {
    fn label(self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::Interrupted => "interrupted",
            Self::Failed => "failed",
            Self::BudgetExhausted => "budget exhausted",
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

/// Outcome of a single batch execution before it is folded into shared state.
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
    /// Wraps a raw batch report together with the seed that produced it.
    pub fn new(seed: u64, batch: BatchReport) -> Self {
        Self { seed, batch }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValueRunStats {
    pub policy: Option<ValuePolicy>,
    pub spent_msg_value: u128,
    pub spent_top_up_value: u128,
    pub msg_value_budget: Option<u128>,
    pub top_up_budget: Option<u128>,
    pub msg_value_overshoot: u128,
    pub top_up_overshoot: u128,
    pub exhausted: Option<BudgetExhaustion>,
}

/// Summary of a whole load-generator run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadRunReport {
    pub metadata: LoadRunMetadata,
    pub ended_by: RunEndedBy,
    pub context: Context,
    pub batch_stats: BatchExecutionStats,
    pub value_stats: Option<ValueRunStats>,
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
        let _ = writeln!(out, "failures observed: {}", stats.failures);

        if let Some(value_stats) = &self.value_stats {
            let _ = writeln!(out, "value accounting: planned reservations");
            let _ = writeln!(
                out,
                "value profile: {}",
                value_stats
                    .policy
                    .as_ref()
                    .and_then(|policy| policy.profile)
                    .map(|profile| profile.to_string())
                    .unwrap_or_else(|| "custom".to_string())
            );
            let _ = writeln!(
                out,
                "planned msg.value: {} ({})",
                value_stats.spent_msg_value,
                format_wei(value_stats.spent_msg_value)
            );
            let _ = writeln!(
                out,
                "planned top-up: {} ({})",
                value_stats.spent_top_up_value,
                format_wvara(value_stats.spent_top_up_value)
            );

            if let Some(budget) = value_stats.msg_value_budget {
                let _ = writeln!(
                    out,
                    "planned msg.value budget: {} ({})",
                    budget,
                    format_wei(budget)
                );
            }
            if let Some(budget) = value_stats.top_up_budget {
                let _ = writeln!(
                    out,
                    "planned top-up budget: {} ({})",
                    budget,
                    format_wvara(budget)
                );
            }

            let _ = writeln!(
                out,
                "planned msg.value overshoot: {} ({})",
                value_stats.msg_value_overshoot,
                format_wei(value_stats.msg_value_overshoot)
            );
            let _ = writeln!(
                out,
                "planned top-up overshoot: {} ({})",
                value_stats.top_up_overshoot,
                format_wvara(value_stats.top_up_overshoot)
            );

            if let Some(exhausted) = value_stats.exhausted {
                let _ = write!(
                    out,
                    "planned budget exhaustion flags: msg.value={}, top-up={}",
                    exhausted.msg_value_exhausted, exhausted.top_up_exhausted
                );
            } else {
                out.pop();
            }
        } else {
            out.pop();
        }

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
    use super::{BatchExecutionStats, LoadRunMetadata, LoadRunReport, RunEndedBy, ValueRunStats};
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
            value_stats: None,
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
            value_stats: None,
        };

        let summary = report.render_pretty();
        assert!(summary.contains("status: completed"));
        assert!(summary.contains("programs: 0 total, 0 active, 0 exited"));
        assert!(summary.contains("claims: 0 requested, 0 succeeded, 0 failed"));
    }

    #[test]
    fn pretty_summary_renders_budget_exhausted_value_stats() {
        use crate::batch::value::{
            BudgetExhaustion, ValuePolicy, ValueProfile, format_wei, format_wvara,
        };

        let report = LoadRunReport {
            metadata: LoadRunMetadata {
                seed: 42,
                workers: 1,
                batch_size: 1,
            },
            ended_by: RunEndedBy::BudgetExhausted,
            context: Context::new(),
            batch_stats: BatchExecutionStats::default(),
            value_stats: Some(ValueRunStats {
                policy: Some(ValuePolicy {
                    profile: Some(ValueProfile::Mainnet),
                    max_msg_value: Some(100_000_000_000_000),
                    max_top_up_value: Some(1_000_000_000_000),
                    total_msg_value_budget: Some(2_000_000_000_000_000),
                    total_top_up_budget: Some(10_000_000_000_000),
                }),
                spent_msg_value: 2_100_000_000_000_000,
                spent_top_up_value: 10_000_000_000_000,
                msg_value_budget: Some(2_000_000_000_000_000),
                top_up_budget: Some(10_000_000_000_000),
                msg_value_overshoot: 100_000_000_000_000,
                top_up_overshoot: 0,
                exhausted: Some(BudgetExhaustion {
                    msg_value_exhausted: true,
                    top_up_exhausted: true,
                }),
            }),
        };

        let summary = report.render_pretty();
        assert!(summary.contains("status: budget exhausted"));
        assert!(summary.contains("value accounting: planned reservations"));
        assert!(summary.contains("value profile: mainnet"));
        assert!(summary.contains("planned msg.value: 2100000000000000"));
        assert!(summary.contains("planned top-up: 10000000000000"));
        assert!(summary.contains("planned msg.value budget: 2000000000000000"));
        assert!(summary.contains("planned top-up budget: 10000000000000"));
        assert!(summary.contains("planned msg.value overshoot: 100000000000000"));
        assert!(summary.contains("planned top-up overshoot: 0"));
        assert!(summary.contains("planned budget exhaustion flags: msg.value=true, top-up=true"));
        assert!(summary.contains(&format!("({})", format_wei(2_100_000_000_000_000))));
        assert!(summary.contains(&format!("({})", format_wvara(10_000_000_000_000))));
        assert!(summary.contains(&format!("({})", format_wei(2_000_000_000_000_000))));
        assert!(summary.contains(&format!("({})", format_wvara(10_000_000_000_000))));
        assert!(summary.contains(&format!("({})", format_wei(100_000_000_000_000))));
        assert!(summary.contains(&format!("({})", format_wvara(0))));
    }
}
