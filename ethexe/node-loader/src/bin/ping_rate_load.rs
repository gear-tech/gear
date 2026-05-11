//! Tiny load runner for the rate-stepping promise-latency experiment.
//!
//! Replays a fixed `PING` payload via `send_transaction_and_watch` against a
//! pre-deployed `demo-ping` mirror, scheduling new sends at a target
//! transactions-per-second rate via `tokio::time::interval`. Each rate step
//! runs for the configured duration and writes per-promise rows to a CSV
//! (`rate_<R>.csv`) under the output directory: `wall_ms,latency_ms,message_id`.
//!
//! Decoupling rate from end-to-end latency lets us see how the cluster
//! handles increasing offered load: each tick spawns a new task instead of
//! blocking on the previous one. In-flight count grows with rate * latency,
//! capped only by tokio's task budget.

// +_+_+ move ping_rate_load to a separate branch.

use anyhow::{Context, Result};
use clap::Parser;
use ethexe_common::Address;
use ethexe_ethereum::EthereumBuilder;
use ethexe_sdk::VaraEthApi;
use gprimitives::ActorId;
use gsigner::secp256k1::{Address as SignerAddress, PrivateKey, Signer};
use std::{
    path::PathBuf,
    str::FromStr,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::{io::AsyncWriteExt, sync::Mutex};

#[derive(Parser, Debug)]
#[command(about = "Rate-stepped injected `PING` load against demo-ping")]
struct Args {
    /// JSON-RPC WS endpoint of an ethexe node (Vara.eth).
    #[arg(long)]
    vara_rpc: String,
    /// JSON-RPC endpoint of the underlying Ethereum node.
    #[arg(long)]
    eth_rpc: String,
    /// Router contract address.
    #[arg(long)]
    router: String,
    /// Sender private key (hex, with or without `0x` prefix).
    #[arg(long)]
    sender_pk: String,
    /// Mirror (program) address that handles `PING`.
    #[arg(long)]
    mirror: String,
    /// Comma-separated list of target tx/s rates.
    #[arg(long, default_value = "1,2,4,8,16,32")]
    rates: String,
    /// Duration of each rate step, in seconds.
    #[arg(long, default_value_t = 300)]
    step_seconds: u64,
    /// Output directory for the per-rate CSV files.
    #[arg(long, default_value = "/tmp")]
    output_dir: PathBuf,
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock before epoch")
        .as_millis()
}

fn signer_from_private_key(private_key_hex: &str) -> Result<(Signer, SignerAddress)> {
    let private_key = PrivateKey::from_str(private_key_hex.trim_start_matches("0x"))
        .context("invalid private key")?;
    let signer = Signer::memory();
    let pubkey = signer.import(private_key)?;
    let address = pubkey.to_address();
    Ok((signer, address))
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    let args = Args::parse();

    let (signer, sender) =
        signer_from_private_key(&args.sender_pk).context("invalid sender private key")?;
    let router = Address::from_str(&args.router).context("invalid router address")?;
    let mirror_addr = Address::from_str(&args.mirror).context("invalid mirror address")?;
    let mirror_actor: ActorId = mirror_addr.into();

    let ethereum = EthereumBuilder::default()
        .rpc_url(args.eth_rpc.clone())
        .router_address(router)
        .signer(signer)
        .sender_address(sender)
        .build()
        .await
        .context("failed to build Ethereum client")?;

    let api = Arc::new(
        VaraEthApi::new(&args.vara_rpc, ethereum)
            .await
            .context("failed to build VaraEthApi")?,
    );

    tokio::fs::create_dir_all(&args.output_dir)
        .await
        .with_context(|| format!("failed to create output dir {:?}", args.output_dir))?;

    let rates: Vec<u32> = args
        .rates
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| {
            s.parse::<u32>()
                .with_context(|| format!("invalid rate {s:?}"))
        })
        .collect::<Result<Vec<_>>>()?;

    eprintln!(
        "starting rate-stepping load: rates={:?}, step={}s, mirror={}",
        rates, args.step_seconds, args.mirror
    );

    for rate in rates {
        run_step(
            api.clone(),
            mirror_actor,
            rate,
            args.step_seconds,
            &args.output_dir,
        )
        .await?;
    }

    Ok(())
}

async fn run_step(
    api: Arc<VaraEthApi>,
    mirror: ActorId,
    rate: u32,
    seconds: u64,
    output_dir: &std::path::Path,
) -> Result<()> {
    let path = output_dir.join(format!("rate_{rate}.csv"));
    let file = tokio::fs::File::create(&path)
        .await
        .with_context(|| format!("failed to create {path:?}"))?;
    let csv = Arc::new(Mutex::new(file));

    let interval_us: u64 = 1_000_000 / rate as u64;
    let mut ticker = tokio::time::interval(Duration::from_micros(interval_us));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    let deadline = Instant::now() + Duration::from_secs(seconds);

    let scheduled = Arc::new(AtomicU64::new(0));
    let ok = Arc::new(AtomicU64::new(0));
    let err = Arc::new(AtomicU64::new(0));

    let mut handles = Vec::new();
    eprintln!("=== rate {rate} tx/s for {seconds}s, csv={path:?} ===");

    while Instant::now() < deadline {
        ticker.tick().await;
        if Instant::now() >= deadline {
            break;
        }
        scheduled.fetch_add(1, Ordering::Relaxed);

        let api = api.clone();
        let csv = csv.clone();
        let ok = ok.clone();
        let err = err.clone();

        handles.push(tokio::spawn(async move {
            let start_wall = now_ms();
            let start = Instant::now();
            match api
                .mirror(mirror)
                .send_message_injected_and_watch(b"PING", 0)
                .await
            {
                Ok((mid, _promise)) => {
                    let elapsed = start.elapsed().as_millis();
                    let line = format!("{start_wall},{elapsed},{mid:?}\n");
                    let mut f = csv.lock().await;
                    let _ = f.write_all(line.as_bytes()).await;
                    ok.fetch_add(1, Ordering::Relaxed);
                }
                Err(e) => {
                    err.fetch_add(1, Ordering::Relaxed);
                    eprintln!("[rate {rate}] error: {e:#}");
                }
            }
        }));
    }

    let pending = handles.len();
    eprintln!(
        "rate {rate}: scheduling phase done; waiting on {pending} in-flight tasks to settle..."
    );
    for h in handles {
        let _ = h.await;
    }

    {
        let mut f = csv.lock().await;
        let _ = f.flush().await;
    }

    eprintln!(
        "rate {rate}: scheduled={}, ok={}, err={}",
        scheduled.load(Ordering::Relaxed),
        ok.load(Ordering::Relaxed),
        err.load(Ordering::Relaxed)
    );
    Ok(())
}
