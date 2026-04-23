use crate::batch::generator::{Batch, BatchWithSeed};
use anyhow::Result;
use clap::ValueEnum;
use gear_call_gen::{
    ClaimValueArgs, CreateProgramArgs, SendMessageArgs, SendReplyArgs, UploadCodeArgs,
    UploadProgramArgs,
};
use gprimitives::ActorId;
use rand::{RngCore, SeedableRng, rngs::SmallRng};
use std::fmt;

pub const DEFAULT_TOP_UP_VALUE: u128 = 500_000_000_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
#[value(rename_all = "lower")]
pub enum ValueProfile {
    Dev,
    Testnet,
    Mainnet,
}

impl fmt::Display for ValueProfile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Dev => "dev",
            Self::Testnet => "testnet",
            Self::Mainnet => "mainnet",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValuePolicy {
    pub profile: Option<ValueProfile>,
    pub max_msg_value: Option<u128>,
    pub max_top_up_value: Option<u128>,
    pub total_msg_value_budget: Option<u128>,
    pub total_top_up_budget: Option<u128>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PlannedSpend {
    pub msg_value: u128,
    pub top_up_value: u128,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BudgetExhaustion {
    pub msg_value_exhausted: bool,
    pub top_up_exhausted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedBatchWithSeed {
    pub seed: u64,
    pub batch: PreparedBatch,
    pub spend: PlannedSpend,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreparedBatch {
    UploadProgram(Vec<PreparedUploadProgram>),
    UploadCode(Vec<UploadCodeArgs>),
    SendMessage(Vec<PreparedSendMessage>),
    CreateProgram(Vec<PreparedCreateProgram>),
    SendReply(Vec<PreparedSendReply>),
    ClaimValue(Vec<ClaimValueArgs>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedUploadProgram {
    pub arg: UploadProgramArgs,
    pub init_value: u128,
    pub top_up_value: u128,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedCreateProgram {
    pub arg: CreateProgramArgs,
    pub init_value: u128,
    pub top_up_value: u128,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedSendMessage {
    pub destination: ActorId,
    pub payload: Vec<u8>,
    pub gas_limit: u64,
    pub use_injected: bool,
    pub value: u128,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedSendReply {
    pub arg: SendReplyArgs,
    pub value: u128,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValueBudgetLedger {
    policy: ValuePolicy,
    spent_msg_value: u128,
    spent_top_up_value: u128,
    exhausted: Option<BudgetExhaustion>,
}

impl ValuePolicy {
    pub fn from_parts(
        profile: Option<ValueProfile>,
        max_msg_value: Option<u128>,
        max_top_up_value: Option<u128>,
        total_msg_value_budget: Option<u128>,
        total_top_up_budget: Option<u128>,
    ) -> Result<Option<Self>> {
        let defaults = match profile {
            Some(ValueProfile::Dev) | None => (None, None, None, None),
            Some(ValueProfile::Testnet) => (
                Some(1_000_000_000_000_000),
                Some(10_000_000_000_000),
                Some(20_000_000_000_000_000),
                Some(100_000_000_000_000),
            ),
            Some(ValueProfile::Mainnet) => (
                Some(100_000_000_000_000),
                Some(1_000_000_000_000),
                Some(2_000_000_000_000_000),
                Some(10_000_000_000_000),
            ),
        };

        let policy = Self {
            profile,
            max_msg_value: max_msg_value.or(defaults.0),
            max_top_up_value: max_top_up_value.or(defaults.1),
            total_msg_value_budget: total_msg_value_budget.or(defaults.2),
            total_top_up_budget: total_top_up_budget.or(defaults.3),
        };

        if policy.max_msg_value.is_none()
            && policy.max_top_up_value.is_none()
            && policy.total_msg_value_budget.is_none()
            && policy.total_top_up_budget.is_none()
        {
            Ok(None)
        } else {
            Ok(Some(policy))
        }
    }

    pub fn describe(&self) -> String {
        format!(
            "profile={}, max_msg_value={}, max_top_up_value={}, total_msg_value_budget={}, total_top_up_budget={}",
            self.profile
                .map(|profile| profile.to_string())
                .unwrap_or_else(|| "none".to_string()),
            self.max_msg_value
                .map(format_wei)
                .unwrap_or_else(|| "disabled".to_string()),
            self.max_top_up_value
                .map(format_wvara)
                .unwrap_or_else(|| "disabled".to_string()),
            self.total_msg_value_budget
                .map(format_wei)
                .unwrap_or_else(|| "disabled".to_string()),
            self.total_top_up_budget
                .map(format_wvara)
                .unwrap_or_else(|| "disabled".to_string()),
        )
    }
}

impl ValueBudgetLedger {
    pub fn new(policy: ValuePolicy) -> Self {
        Self {
            policy,
            spent_msg_value: 0,
            spent_top_up_value: 0,
            exhausted: None,
        }
    }

    pub fn policy(&self) -> &ValuePolicy {
        &self.policy
    }

    pub fn spent_msg_value(&self) -> u128 {
        self.spent_msg_value
    }

    pub fn spent_top_up_value(&self) -> u128 {
        self.spent_top_up_value
    }

    pub fn exhaustion(&self) -> Option<BudgetExhaustion> {
        self.exhausted
    }

    pub fn is_exhausted(&self) -> bool {
        self.exhausted.is_some()
    }

    pub fn reserve(&mut self, spend: PlannedSpend) -> Option<BudgetExhaustion> {
        self.spent_msg_value = self.spent_msg_value.saturating_add(spend.msg_value);
        self.spent_top_up_value = self.spent_top_up_value.saturating_add(spend.top_up_value);

        let exhaustion = BudgetExhaustion {
            msg_value_exhausted: self
                .policy
                .total_msg_value_budget
                .is_some_and(|budget| self.spent_msg_value >= budget),
            top_up_exhausted: self
                .policy
                .total_top_up_budget
                .is_some_and(|budget| self.spent_top_up_value >= budget),
        };

        if exhaustion.msg_value_exhausted || exhaustion.top_up_exhausted {
            self.exhausted = Some(exhaustion);
        }

        self.exhausted
    }
}

pub fn prepare_batch(
    batch_with_seed: BatchWithSeed,
    policy: Option<&ValuePolicy>,
) -> PreparedBatchWithSeed {
    let (seed, batch) = batch_with_seed.into();
    let mut rng = SmallRng::seed_from_u64(seed);

    match batch {
        Batch::UploadProgram(args) => {
            let mut spend = PlannedSpend::default();
            let prepared = args
                .into_iter()
                .map(|arg| {
                    let init_value = clamp_msg_value(fuzz_message_value(&mut rng), policy);
                    let top_up_value = clamp_top_up_value(DEFAULT_TOP_UP_VALUE, policy);
                    spend.msg_value = spend.msg_value.saturating_add(init_value);
                    spend.top_up_value = spend.top_up_value.saturating_add(top_up_value);
                    PreparedUploadProgram {
                        arg,
                        init_value,
                        top_up_value,
                    }
                })
                .collect();

            PreparedBatchWithSeed {
                seed,
                batch: PreparedBatch::UploadProgram(prepared),
                spend,
            }
        }
        Batch::UploadCode(args) => PreparedBatchWithSeed {
            seed,
            batch: PreparedBatch::UploadCode(args),
            spend: PlannedSpend::default(),
        },
        Batch::SendMessage(args) => {
            let mut spend = PlannedSpend::default();
            let prepared = args
                .into_iter()
                .map(|arg| {
                    let SendMessageArgs((destination, payload, gas_limit, _)) = arg;
                    let use_injected = prefer_injected_tx(&mut rng);
                    let value = if use_injected {
                        0
                    } else {
                        clamp_msg_value(fuzz_message_value(&mut rng), policy)
                    };
                    spend.msg_value = spend.msg_value.saturating_add(value);
                    PreparedSendMessage {
                        destination,
                        payload,
                        gas_limit,
                        use_injected,
                        value,
                    }
                })
                .collect();

            PreparedBatchWithSeed {
                seed,
                batch: PreparedBatch::SendMessage(prepared),
                spend,
            }
        }
        Batch::CreateProgram(args) => {
            let mut spend = PlannedSpend::default();
            let prepared = args
                .into_iter()
                .map(|arg| {
                    let init_value = clamp_msg_value(fuzz_message_value(&mut rng), policy);
                    let top_up_value = clamp_top_up_value(DEFAULT_TOP_UP_VALUE, policy);
                    spend.msg_value = spend.msg_value.saturating_add(init_value);
                    spend.top_up_value = spend.top_up_value.saturating_add(top_up_value);
                    PreparedCreateProgram {
                        arg,
                        init_value,
                        top_up_value,
                    }
                })
                .collect();

            PreparedBatchWithSeed {
                seed,
                batch: PreparedBatch::CreateProgram(prepared),
                spend,
            }
        }
        Batch::SendReply(args) => {
            let mut spend = PlannedSpend::default();
            let prepared = args
                .into_iter()
                .map(|arg| {
                    let value = clamp_msg_value(fuzz_message_value(&mut rng), policy);
                    spend.msg_value = spend.msg_value.saturating_add(value);
                    PreparedSendReply { arg, value }
                })
                .collect();

            PreparedBatchWithSeed {
                seed,
                batch: PreparedBatch::SendReply(prepared),
                spend,
            }
        }
        Batch::ClaimValue(args) => PreparedBatchWithSeed {
            seed,
            batch: PreparedBatch::ClaimValue(args),
            spend: PlannedSpend::default(),
        },
    }
}

fn clamp_msg_value(value: u128, policy: Option<&ValuePolicy>) -> u128 {
    policy
        .and_then(|policy| policy.max_msg_value)
        .map_or(value, |cap| value.min(cap))
}

fn clamp_top_up_value(value: u128, policy: Option<&ValuePolicy>) -> u128 {
    policy
        .and_then(|policy| policy.max_top_up_value)
        .map_or(value, |cap| value.min(cap))
}

fn fuzz_message_value(rng: &mut impl RngCore) -> u128 {
    if rng.next_u32() % 10 < 6 {
        return 0;
    }

    let max_value = 1_000_000_000_000_000_000u128;
    let random_value = ((rng.next_u64() as u128) << 64) | (rng.next_u64() as u128);
    random_value % max_value
}

fn prefer_injected_tx(rng: &mut impl RngCore) -> bool {
    (rng.next_u32() % 10) < 7
}

pub fn format_wei(value: u128) -> String {
    format_amount(value, 18, "ETH")
}

pub fn format_wvara(value: u128) -> String {
    format_amount(value, 12, "WVARA")
}

pub fn format_amount(value: u128, decimals: u32, symbol: &str) -> String {
    let unit = 10u128.pow(decimals);
    let int_part = value / unit;
    let frac_part = value % unit;

    if frac_part == 0 {
        return format!("{int_part} {symbol}");
    }

    let frac = format!("{frac_part:0width$}", width = decimals as usize)
        .trim_end_matches('0')
        .to_string();

    format!("{int_part}.{frac} {symbol}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::batch::generator::Batch;
    use gear_call_gen::{CreateProgramArgs, SendReplyArgs};
    use gprimitives::{CodeId, MessageId};

    fn code(seed: u8) -> CodeId {
        CodeId::from([seed; 32])
    }

    fn message(seed: u8) -> MessageId {
        MessageId::from([seed; 32])
    }

    #[test]
    fn mainnet_profile_applies_defaults() {
        let policy = ValuePolicy::from_parts(Some(ValueProfile::Mainnet), None, None, None, None)
            .expect("policy")
            .expect("enabled");

        assert_eq!(policy.profile, Some(ValueProfile::Mainnet));
        assert_eq!(policy.max_msg_value, Some(100_000_000_000_000));
        assert_eq!(policy.max_top_up_value, Some(1_000_000_000_000));
        assert_eq!(policy.total_msg_value_budget, Some(2_000_000_000_000_000));
        assert_eq!(policy.total_top_up_budget, Some(10_000_000_000_000));
    }

    #[test]
    fn explicit_overrides_win_over_profile_defaults() {
        let policy = ValuePolicy::from_parts(
            Some(ValueProfile::Testnet),
            Some(7),
            Some(11),
            Some(13),
            Some(17),
        )
        .expect("policy")
        .expect("enabled");

        assert_eq!(policy.max_msg_value, Some(7));
        assert_eq!(policy.max_top_up_value, Some(11));
        assert_eq!(policy.total_msg_value_budget, Some(13));
        assert_eq!(policy.total_top_up_budget, Some(17));
    }

    #[test]
    fn prepare_batch_caps_program_and_reply_values() {
        let policy = ValuePolicy::from_parts(None, Some(5), Some(7), None, None)
            .expect("policy")
            .expect("enabled");

        let create = Batch::CreateProgram(vec![CreateProgramArgs((
            code(1),
            vec![1, 2, 3],
            vec![4, 5, 6],
            1_000,
            0,
        ))]);
        let prepared_create = prepare_batch((11_u64, create).into(), Some(&policy));
        assert_eq!(prepared_create.spend.msg_value, 5);
        assert_eq!(prepared_create.spend.top_up_value, 7);

        let reply = Batch::SendReply(vec![SendReplyArgs((
            message(9),
            vec![7, 8, 9],
            1_000,
            0,
        ))]);
        let prepared_reply = prepare_batch((12_u64, reply).into(), Some(&policy));
        assert_eq!(prepared_reply.spend.msg_value, 5);
        assert_eq!(prepared_reply.spend.top_up_value, 0);
    }

    #[test]
    fn prepare_batch_is_seed_deterministic_when_policy_is_disabled() {
        let batch = Batch::SendReply(vec![SendReplyArgs((
            message(7),
            vec![1, 2, 3],
            1_000,
            0,
        ))]);

        let first = prepare_batch((77_u64, batch.clone()).into(), None);
        let second = prepare_batch((77_u64, batch).into(), None);

        assert_eq!(first, second);
    }

    #[test]
    fn ledger_marks_exhaustion_after_overshooting_reservation() {
        let policy = ValuePolicy::from_parts(None, None, None, Some(5), Some(7))
            .expect("policy")
            .expect("enabled");
        let mut ledger = ValueBudgetLedger::new(policy);

        let exhaustion = ledger.reserve(PlannedSpend {
            msg_value: 6,
            top_up_value: 7,
        });

        assert_eq!(
            exhaustion,
            Some(BudgetExhaustion {
                msg_value_exhausted: true,
                top_up_exhausted: true,
            })
        );
        assert!(ledger.is_exhausted());
    }
}
