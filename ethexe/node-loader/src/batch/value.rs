use crate::batch::generator::{Batch, BatchWithSeed};
use anyhow::Result;
use clap::ValueEnum;
use gear_call_gen::{
    ClaimValueArgs, CreateProgramArgs, SendMessageArgs, SendReplyArgs, UploadCodeArgs,
    UploadProgramArgs,
};
use rand::{RngCore, SeedableRng, rngs::SmallRng};
use std::fmt;

pub(crate) const DEFAULT_TOP_UP_VALUE: u128 = 500_000_000_000_000;

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
    pub top_up_value: u128,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedCreateProgram {
    pub arg: CreateProgramArgs,
    pub top_up_value: u128,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedSendMessage {
    pub arg: SendMessageArgs,
    pub use_injected: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedSendReply {
    pub arg: SendReplyArgs,
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

        if profile.is_none()
            && policy.max_msg_value.is_none()
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
                .is_some_and(|budget| spend.msg_value > 0 && self.spent_msg_value >= budget),
            top_up_exhausted: self
                .policy
                .total_top_up_budget
                .is_some_and(|budget| spend.top_up_value > 0 && self.spent_top_up_value >= budget),
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
                    let arg = set_upload_program_value(arg, init_value);
                    PreparedUploadProgram { arg, top_up_value }
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
                    let fuzzed_value = fuzz_message_value(&mut rng);
                    let use_injected = prefer_injected_tx(&mut rng);
                    let value = if use_injected {
                        0
                    } else {
                        clamp_msg_value(fuzzed_value, policy)
                    };
                    spend.msg_value = spend.msg_value.saturating_add(value);
                    let arg = SendMessageArgs((destination, payload, gas_limit, value));
                    PreparedSendMessage { arg, use_injected }
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
                    let arg = set_create_program_value(arg, init_value);
                    PreparedCreateProgram { arg, top_up_value }
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
                    let arg = set_send_reply_value(arg, value);
                    PreparedSendReply { arg }
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

pub(crate) fn fuzz_message_value(rng: &mut impl RngCore) -> u128 {
    if rng.next_u32() % 10 < 6 {
        return 0;
    }

    let max_value = 1_000_000_000_000_000_000u128;
    let random_value = ((rng.next_u64() as u128) << 64) | (rng.next_u64() as u128);
    random_value % max_value
}

pub(crate) fn prefer_injected_tx(rng: &mut impl RngCore) -> bool {
    (rng.next_u32() % 10) < 7
}

fn set_upload_program_value(arg: UploadProgramArgs, value: u128) -> UploadProgramArgs {
    let UploadProgramArgs((code, salt, payload, gas_limit, _)) = arg;
    UploadProgramArgs((code, salt, payload, gas_limit, value))
}

fn set_create_program_value(arg: CreateProgramArgs, value: u128) -> CreateProgramArgs {
    let CreateProgramArgs((code_id, salt, payload, gas_limit, _)) = arg;
    CreateProgramArgs((code_id, salt, payload, gas_limit, value))
}

fn set_send_reply_value(arg: SendReplyArgs, value: u128) -> SendReplyArgs {
    let SendReplyArgs((message_id, payload, gas_limit, _)) = arg;
    SendReplyArgs((message_id, payload, gas_limit, value))
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
    use gear_call_gen::{CreateProgramArgs, SendMessageArgs, SendReplyArgs, UploadProgramArgs};
    use gprimitives::{ActorId, CodeId, MessageId};
    use rand::{SeedableRng, rngs::SmallRng};

    fn code(seed: u8) -> CodeId {
        CodeId::from([seed; 32])
    }

    fn actor(seed: u8) -> ActorId {
        ActorId::from([seed; 32])
    }

    fn message(seed: u8) -> MessageId {
        MessageId::from([seed; 32])
    }

    fn upload_program_args(seed: u8) -> UploadProgramArgs {
        UploadProgramArgs((vec![seed; 32], vec![1, 2, 3], vec![4, 5, 6], 1_000, 0))
    }

    fn create_program_args(seed: u8) -> CreateProgramArgs {
        CreateProgramArgs((code(seed), vec![1, 2, 3], vec![4, 5, 6], 1_000, 0))
    }

    fn send_reply_args(seed: u8) -> SendReplyArgs {
        SendReplyArgs((message(seed), vec![7, 8, 9], 1_000, 0))
    }

    fn send_message_args(seed: u8) -> SendMessageArgs {
        SendMessageArgs((actor(seed), vec![1, 2, 3], 1_000, 0))
    }

    fn send_message_batch() -> Batch {
        Batch::SendMessage(vec![send_message_args(1), send_message_args(2)])
    }

    fn prepare_send_message_executor(
        seed: u64,
        policy: Option<&ValuePolicy>,
    ) -> PreparedBatchWithSeed {
        let mut rng = SmallRng::seed_from_u64(seed);
        let mut spend = PlannedSpend::default();
        let prepared = vec![send_message_args(1), send_message_args(2)]
            .into_iter()
            .map(|arg| {
                let SendMessageArgs((destination, payload, gas_limit, _)) = arg;
                let fuzzed_value = fuzz_message_value(&mut rng);
                let use_injected = prefer_injected_tx(&mut rng);
                let value = if use_injected {
                    0
                } else {
                    clamp_msg_value(fuzzed_value, policy)
                };
                spend.msg_value = spend.msg_value.saturating_add(value);
                PreparedSendMessage {
                    arg: SendMessageArgs((destination, payload, gas_limit, value)),
                    use_injected,
                }
            })
            .collect();

        PreparedBatchWithSeed {
            seed,
            batch: PreparedBatch::SendMessage(prepared),
            spend,
        }
    }

    fn prepare_send_message_legacy(
        seed: u64,
        policy: Option<&ValuePolicy>,
    ) -> PreparedBatchWithSeed {
        let mut rng = SmallRng::seed_from_u64(seed);
        let mut spend = PlannedSpend::default();
        let prepared = vec![send_message_args(1), send_message_args(2)]
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
                    arg: SendMessageArgs((destination, payload, gas_limit, value)),
                    use_injected,
                }
            })
            .collect();

        PreparedBatchWithSeed {
            seed,
            batch: PreparedBatch::SendMessage(prepared),
            spend,
        }
    }

    fn send_message_order_mismatch_seed(policy: Option<&ValuePolicy>) -> u64 {
        for seed in 0..1_000_u64 {
            if prepare_send_message_executor(seed, policy)
                != prepare_send_message_legacy(seed, policy)
            {
                return seed;
            }
        }

        panic!("expected to find a divergent send_message seed");
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
    fn explicit_dev_profile_is_preserved() {
        let policy = ValuePolicy::from_parts(Some(ValueProfile::Dev), None, None, None, None)
            .expect("policy")
            .expect("enabled");

        assert_eq!(policy.profile, Some(ValueProfile::Dev));
        assert!(policy.max_msg_value.is_none());
        assert!(policy.max_top_up_value.is_none());
        assert!(policy.total_msg_value_budget.is_none());
        assert!(policy.total_top_up_budget.is_none());
    }

    #[test]
    fn prepare_batch_caps_program_and_reply_values() {
        let policy = ValuePolicy::from_parts(None, Some(5), Some(7), None, None)
            .expect("policy")
            .expect("enabled");

        let upload = Batch::UploadProgram(vec![upload_program_args(1)]);
        let prepared_upload = prepare_batch((10_u64, upload).into(), Some(&policy));
        assert_eq!(prepared_upload.spend.msg_value, 5);
        assert_eq!(prepared_upload.spend.top_up_value, 7);
        let PreparedBatch::UploadProgram(items) = prepared_upload.batch else {
            panic!("unexpected batch variant");
        };
        let UploadProgramArgs((_, _, _, _, value)) = items[0].arg.clone();
        assert_eq!(value, 5);
        assert_eq!(items[0].top_up_value, 7);

        let create = Batch::CreateProgram(vec![create_program_args(1)]);
        let prepared_create = prepare_batch((11_u64, create).into(), Some(&policy));
        assert_eq!(prepared_create.spend.msg_value, 5);
        assert_eq!(prepared_create.spend.top_up_value, 7);
        let PreparedBatch::CreateProgram(items) = prepared_create.batch else {
            panic!("unexpected batch variant");
        };
        let CreateProgramArgs((_, _, _, _, value)) = items[0].arg.clone();
        assert_eq!(value, 5);
        assert_eq!(items[0].top_up_value, 7);

        let reply = Batch::SendReply(vec![send_reply_args(9)]);
        let prepared_reply = prepare_batch((12_u64, reply).into(), Some(&policy));
        assert_eq!(prepared_reply.spend.top_up_value, 0);
        let PreparedBatch::SendReply(items) = prepared_reply.batch else {
            panic!("unexpected batch variant");
        };
        let SendReplyArgs((_, _, _, value)) = items[0].arg.clone();
        assert_eq!(value, prepared_reply.spend.msg_value);
    }

    #[test]
    fn prepare_batch_is_seed_deterministic_when_policy_is_disabled() {
        let first = prepare_batch(
            (77_u64, Batch::SendReply(vec![send_reply_args(7)])).into(),
            None,
        );
        let second = prepare_batch(
            (77_u64, Batch::SendReply(vec![send_reply_args(7)])).into(),
            None,
        );

        assert_eq!(first, second);
    }

    #[test]
    fn prepare_batch_send_message_consumes_rng_in_executor_order() {
        let seed = send_message_order_mismatch_seed(None);
        let prepared = prepare_batch((seed, send_message_batch()).into(), None);
        let expected = prepare_send_message_executor(seed, None);
        let legacy = prepare_send_message_legacy(seed, None);

        assert_eq!(prepared, expected);
        assert_ne!(prepared, legacy);
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

    #[test]
    fn ledger_zero_budget_with_zero_spend_does_not_exhaust() {
        let policy = ValuePolicy::from_parts(None, None, None, Some(0), Some(0))
            .expect("policy")
            .expect("enabled");
        let mut ledger = ValueBudgetLedger::new(policy);

        let exhaustion = ledger.reserve(PlannedSpend::default());

        assert_eq!(exhaustion, None);
        assert!(!ledger.is_exhausted());
    }

    #[test]
    fn ledger_zero_budget_with_positive_spend_exhausts() {
        let policy = ValuePolicy::from_parts(None, None, None, Some(0), Some(0))
            .expect("policy")
            .expect("enabled");
        let mut ledger = ValueBudgetLedger::new(policy);

        let exhaustion = ledger.reserve(PlannedSpend {
            msg_value: 1,
            top_up_value: 0,
        });

        assert_eq!(
            exhaustion,
            Some(BudgetExhaustion {
                msg_value_exhausted: true,
                top_up_exhausted: false,
            })
        );
        assert!(ledger.is_exhausted());
    }

    #[test]
    fn ledger_exact_budget_exhausts_on_positive_reservation() {
        let policy = ValuePolicy::from_parts(None, None, None, Some(5), Some(7))
            .expect("policy")
            .expect("enabled");
        let mut ledger = ValueBudgetLedger::new(policy);

        assert_eq!(
            ledger.reserve(PlannedSpend {
                msg_value: 5,
                top_up_value: 7,
            }),
            Some(BudgetExhaustion {
                msg_value_exhausted: true,
                top_up_exhausted: true,
            })
        );
        assert!(ledger.is_exhausted());
    }
}
