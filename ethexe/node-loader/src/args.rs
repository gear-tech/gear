//! CLI args for the `ethexe-node-loader`

use crate::batch::value::ValueProfile;
use anyhow::{Error, anyhow};
use clap::{ArgAction, Parser};
use std::str::FromStr;

#[derive(Debug, Parser)]
#[command(
    name = "ethexe-node-loader",
    about = "Load-testing tool for an ethexe dev node",
    long_about = "ethexe-node-loader generates randomized workloads against an ethexe dev node. It can upload code/programs, send messages, send replies, and claim values in batches to stress-test the node.\n\nUse `load` for continuous traffic generation, `fuzz` to exercise syscalls via a mega contract, and `dump` to generate a wasm program from a seed for debugging."
)]
pub enum Params {
    /// Dump the wasm program with provided seed to "out.wasm"
    Dump {
        /// Seed value used to generate program via wasm-gen
        seed: u64,
    },
    /// Perform load test on the node
    Load(Box<LoadParams>),
    /// Fuzz-test syscalls via the mega contract
    Fuzz(FuzzParams),
}

/// Parameters for the continuous load-generation mode.
///
/// Most defaults assume a local Anvil + `ethexe run --dev` setup.
#[derive(Debug, Parser)]
pub struct LoadParams {
    /// Ethexe node
    #[arg(long, default_value = "ws://localhost:8545")]
    pub node: String,
    #[arg(
        long = "ethexe-node",
        value_delimiter = ',',
        num_args = 1..,
        default_value = "ws://localhost:9944"
    )]
    pub ethexe_nodes: Vec<String>,

    /// Router address to send messages into.
    #[arg(long, default_value = "0xCf7Ed3AccA5a467e9e704C703E8D87F634fB0Fc9")]
    pub router_address: String,
    /// WVara proxy address to work with tokens.
    #[arg(long, default_value = "0x84eA74d481Ee0A5332c457a4d796187F6Ba67fEB")]
    pub wvara_address: String,

    /// Percentage added on top of the default EIP-1559 fee estimate (e.g. 20 = +20 %).
    #[arg(long, default_value_t = 20, env = "EIP1559_FEE_INCREASE_PERCENTAGE")]
    pub eip1559_fee_increase_percentage: u64,

    /// Multiplier for blob gas price estimation (EIP-4844 side of blob-carrying txs).
    #[arg(long, default_value_t = 6, env = "BLOB_GAS_MULTIPLIER")]
    pub blob_gas_multiplier: u128,

    /// A private key for sender account.
    #[arg(long, env = "SENDER_PRIVATE_KEY")]
    pub sender_private_key: Option<String>,
    #[arg(long, env = "SENDER_ADDRESS")]
    pub sender_address: Option<String>,

    #[arg(long)]
    pub loader_seed: Option<u64>,
    #[arg(long)]
    pub code_seed_type: Option<SeedVariant>,
    /// Desirable amount of workers in task pool, bounded by available prebuilt Anvil accounts.
    #[arg(long, short, default_value = "1")]
    pub workers: usize,
    /// Private keys for worker accounts. Repeat the flag once per worker.
    #[arg(
        long = "worker-private-key",
        env = "WORKER_PRIVATE_KEYS",
        value_delimiter = ','
    )]
    pub worker_private_keys: Vec<String>,
    #[arg(long, short, default_value = "1")]
    pub batch_size: usize,
    /// Percentage of load batches that create new programs after bootstrapping.
    ///
    /// The default preserves the historical mix: `upload_program` and
    /// `create_program` were two out of six uniformly selected batch families.
    #[arg(long, value_parser = clap::value_parser!(u8).range(0..=100))]
    pub program_creation_ratio: Option<u8>,
    /// Whether to batch regular `send_message` calls through the multicall contract.
    #[arg(long, default_value_t = true, action = ArgAction::Set)]
    pub use_send_message_multicall: bool,
    /// Existing BatchMulticall contract address to reuse instead of deploying a new one.
    #[arg(long)]
    pub send_message_multicall_address: Option<String>,
    /// Value policy preset for load-mode message and top-up amounts.
    #[arg(long, ignore_case = true, value_enum)]
    pub value_profile: Option<ValueProfile>,
    /// Per-message value cap in wei.
    #[arg(long)]
    pub max_msg_value: Option<u128>,
    /// Per-program top-up cap in WVARA base units.
    #[arg(long)]
    pub max_top_up_value: Option<u128>,
    /// Total message value budget across the run in wei.
    #[arg(long)]
    pub total_msg_value_budget: Option<u128>,
    /// Total top-up budget across the run in WVARA base units.
    #[arg(long)]
    pub total_top_up_budget: Option<u128>,
    /// Target WVARA balance per worker before load starts. Defaults to a value-policy-derived
    /// amount, or the dev-mode fallback when value spending is uncapped.
    #[arg(long, env = "MINT_AMOUNT")]
    pub mint_amount: Option<u128>,
}

/// Parses CLI arguments for the binary and returns the selected subcommand.
pub fn parse_cli_params() -> Params {
    Params::parse()
}

/// Parameters for the syscall fuzzing mode.
#[derive(Debug, Parser)]
pub struct FuzzParams {
    /// Ethereum RPC node endpoint
    #[arg(long, default_value = "ws://localhost:8545")]
    pub node: String,
    /// Ethexe sequencer node endpoint
    #[arg(long, default_value = "ws://localhost:9944")]
    pub ethexe_node: String,

    /// Router address to send messages into.
    #[arg(long, default_value = "0xCf7Ed3AccA5a467e9e704C703E8D87F634fB0Fc9")]
    pub router_address: String,

    /// A private key for sender account.
    #[arg(long, env = "SENDER_PRIVATE_KEY")]
    pub sender_private_key: Option<String>,

    /// Seed for the random fuzz command generator.
    #[arg(long)]
    pub seed: Option<u64>,

    /// Number of fuzz iterations (0 = infinite).
    #[arg(long, default_value = "100")]
    pub iterations: u64,

    /// Maximum commands per message sent to the mega contract.
    #[arg(long, default_value = "8")]
    pub max_commands: usize,
}

/// Controls how seeds for generated WASM programs are produced.
///
/// `Dynamic` advances an RNG stream starting from the provided value, while
/// `Constant` keeps generating the same seed every time for easier repros.
#[derive(Debug, Clone)]
pub enum SeedVariant {
    // TODO remove later (considering)
    Dynamic(u64),
    Constant(u64),
}

impl FromStr for SeedVariant {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.to_lowercase();
        let (variant, num_str) = s.split_once('=').ok_or_else(|| {
            anyhow!("Invalid seed argument format {s:?}. Must be 'seed_variant=num'")
        })?;

        let num = num_str.parse::<u64>()?;
        match variant {
            "start" => Ok(SeedVariant::Dynamic(num)),
            "constant" => Ok(SeedVariant::Constant(num)),
            v => Err(anyhow!("Invalid variant {v}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::batch::value::ValueProfile;
    use clap::Parser;

    #[test]
    fn load_params_parse_value_profile_and_overrides() {
        let params = Params::try_parse_from([
            "ethexe-node-loader",
            "load",
            "--value-profile",
            "mainnet",
            "--max-msg-value",
            "123",
            "--max-top-up-value",
            "456",
            "--total-msg-value-budget",
            "789",
            "--total-top-up-budget",
            "999",
            "--mint-amount",
            "111",
            "--send-message-multicall-address",
            "0x1111111111111111111111111111111111111111",
        ])
        .expect("parse");

        let Params::Load(load_params) = params else {
            panic!("expected load params");
        };

        assert_eq!(load_params.value_profile, Some(ValueProfile::Mainnet));
        assert_eq!(load_params.max_msg_value, Some(123));
        assert_eq!(load_params.max_top_up_value, Some(456));
        assert_eq!(load_params.total_msg_value_budget, Some(789));
        assert_eq!(load_params.total_top_up_budget, Some(999));
        assert_eq!(load_params.mint_amount, Some(111));
        assert_eq!(
            load_params.send_message_multicall_address.as_deref(),
            Some("0x1111111111111111111111111111111111111111")
        );
    }

    #[test]
    fn load_params_default_ethereum_fee_options() {
        let params = Params::try_parse_from(["ethexe-node-loader", "load"]).expect("parse");
        let Params::Load(load_params) = params else {
            panic!("expected load params");
        };
        assert_eq!(load_params.eip1559_fee_increase_percentage, 20);
        assert_eq!(load_params.blob_gas_multiplier, 6);
    }

    #[test]
    fn load_params_custom_ethereum_fee_options() {
        let params = Params::try_parse_from([
            "ethexe-node-loader",
            "load",
            "--eip1559-fee-increase-percentage",
            "30",
            "--blob-gas-multiplier",
            "3",
        ])
        .expect("parse");
        let Params::Load(load_params) = params else {
            panic!("expected load params");
        };
        assert_eq!(load_params.eip1559_fee_increase_percentage, 30);
        assert_eq!(load_params.blob_gas_multiplier, 3);
    }

    #[test]
    fn load_params_accept_zero_caps_and_budgets() {
        let params = Params::try_parse_from([
            "ethexe-node-loader",
            "load",
            "--max-msg-value",
            "0",
            "--max-top-up-value",
            "0",
            "--total-msg-value-budget",
            "0",
            "--total-top-up-budget",
            "0",
        ])
        .expect("parse");

        let Params::Load(load_params) = params else {
            panic!("expected load params");
        };

        assert_eq!(load_params.max_msg_value, Some(0));
        assert_eq!(load_params.max_top_up_value, Some(0));
        assert_eq!(load_params.total_msg_value_budget, Some(0));
        assert_eq!(load_params.total_top_up_budget, Some(0));
    }

    #[test]
    fn load_params_parse_multiple_worker_private_keys() {
        let params = Params::try_parse_from([
            "ethexe-node-loader",
            "load",
            "--workers",
            "2",
            "--worker-private-key",
            "0x1111",
            "--worker-private-key",
            "0x2222",
        ])
        .expect("parse");

        let Params::Load(load_params) = params else {
            panic!("expected load params");
        };

        assert_eq!(load_params.workers, 2);
        assert_eq!(load_params.worker_private_keys, vec!["0x1111", "0x2222"]);
    }
}
