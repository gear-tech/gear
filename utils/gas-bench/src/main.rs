// Deterministic gas-burn benchmark harness for Gear programs.
//
// Usage: gas-bench <scenario> --wasm <path/to/program.opt.wasm>
//
// Each scenario builds a fresh `gtest::System`, deploys the program under
// test plus any required mock counter-parties, sends a representative
// message, runs blocks until the queue drains, and prints the total
// `gas_burned` summed across every message produced by the scenario.
//
// Numbers are deterministic across runs of the same wasm: any non-zero
// delta between two runs is a real cost change.
//
// Recipe for an A/B comparison:
//   git switch <baseline>; cargo build -p demo-async --release
//   cp target/wasm32-gear/release/demo_async.opt.wasm /tmp/baseline.wasm
//   git switch <candidate>; cargo build -p demo-async --release
//   cp target/wasm32-gear/release/demo_async.opt.wasm /tmp/candidate.wasm
//   cargo run -p gas-bench --release -- async-common --wasm /tmp/baseline.wasm
//   cargo run -p gas-bench --release -- async-common --wasm /tmp/candidate.wasm

mod scenarios;

use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Scenario to run.
    #[command(subcommand)]
    scenario: Scenario,
}

#[derive(clap::Subcommand, Debug)]
enum Scenario {
    /// `demo-async` Common: 3× send_for_reply + reply, exercises async
    /// runtime, load_bytes and handle_reply_with_hook paths.
    AsyncCommon {
        #[arg(long)]
        wasm: PathBuf,
    },
    /// `demo-async` Mutex: lock + send_for_reply + reply, exercises
    /// mutex-future + lock storage + async runtime.
    AsyncMutex {
        #[arg(long)]
        wasm: PathBuf,
    },
    /// `demo-ping` PING/PONG: pure sync handle path; no async runtime
    /// touched. Useful as a control/baseline for sync-only workloads.
    SyncPing {
        #[arg(long)]
        wasm: PathBuf,
    },
}

fn main() {
    let args = Args::parse();
    let result = match args.scenario {
        Scenario::AsyncCommon { wasm } => scenarios::async_common(&wasm),
        Scenario::AsyncMutex { wasm } => scenarios::async_mutex(&wasm),
        Scenario::SyncPing { wasm } => scenarios::sync_ping(&wasm),
    };
    println!("scenario:           {}", result.name);
    println!("wasm:               {}", result.wasm.display());
    println!("messages_processed: {}", result.messages);
    println!("total_gas_burned:   {}", result.total_gas);
    if !result.per_message.is_empty() {
        println!("per_message:");
        for (i, gas) in result.per_message.iter().enumerate() {
            println!("  [{i:>2}] {gas}");
        }
    }
}
