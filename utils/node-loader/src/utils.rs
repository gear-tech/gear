use crate::SmallRng;
use anyhow::{anyhow, Result};
use futures::Future;
use futures_timer::Delay;
use gclient::WSAddress;
use gear_call_gen::GearProgGenConfig;
use reqwest::Client;
use std::{
    collections::HashMap, fs::File, io::Write, iter, result::Result as StdResult, time::Duration,
};

/// subxt's GenericError::Rpc::RequestError::RestartNeeded
pub const SUBXT_RPC_REQUEST_ERR_STR: &str = "Rpc error: The background task been terminated because: Networking or low-level protocol error";
/// subxt's GenericError::Rpc::RequestError::Call (CallError::Failed)
pub const SUBXT_RPC_CALL_ERR_STR: &str = "Transaction would exhaust the block limits";
pub const EVENTS_TIMEOUT_ERR_STR: &str = "Block events timeout";
pub const TRANSACTION_INVALID: &str = "Transaction Invalid";
pub const TRANSACTION_DROPPED: &str = "Transaction Dropped";
pub const WAITING_TX_FINALIZED_TIMEOUT_ERR_STR: &str =
    "Transaction finalization wait timeout is reached";

pub fn dump_with_seed(seed: u64) -> Result<()> {
    let code = gear_call_gen::generate_gear_program::<SmallRng>(
        seed,
        GearProgGenConfig::new_normal(),
        Default::default(),
    );

    let mut file = File::create("out.wasm")?;
    file.write_all(&code)?;

    Ok(())
}

pub fn str_to_wsaddr(endpoint: String) -> WSAddress {
    let endpoint = endpoint.replace("://", ":");

    let mut addr_parts = endpoint.split(':');

    let domain = format!(
        "{}://{}",
        addr_parts.next().unwrap_or("ws"),
        addr_parts.next().unwrap_or("127.0.0.1")
    );
    let port = addr_parts.next().and_then(|v| v.parse().ok());

    WSAddress::new(domain, port)
}

pub fn iterator_with_args<T, F: FnMut() -> T>(
    max_size: usize,
    mut args: F,
) -> impl Iterator<Item = T> {
    let mut size = 0;
    iter::from_fn(move || {
        if size >= max_size {
            return None;
        }

        size += 1;

        Some(args())
    })
}

pub fn convert_iter<V, T: Into<V> + Clone>(args: Vec<T>) -> impl IntoIterator<Item = V> + Clone {
    args.into_iter().map(Into::into)
}

pub trait SwapResult {
    type SwappedOk;
    type SwappedErr;

    fn swap_result(self) -> StdResult<Self::SwappedOk, Self::SwappedErr>;
}

impl<T, E> SwapResult for StdResult<T, E> {
    type SwappedOk = E;
    type SwappedErr = T;

    fn swap_result(self) -> StdResult<Self::SwappedOk, Self::SwappedErr> {
        match self {
            Ok(t) => Err(t),
            Err(e) => Ok(e),
        }
    }
}

pub async fn with_timeout<T>(fut: impl Future<Output = T>) -> Result<T> {
    // 5 minute as default
    let wait_task = Delay::new(Duration::from_millis(5 * 60 * 1_000));

    tokio::select! {
        output = fut => Ok(output),
        _ = wait_task => {
            Err(anyhow!("Timeout occurred while running the action"))
        }
    }
}

pub async fn stop_node(monitor_url: String) -> Result<()> {
    let client = Client::new();
    let mut params = HashMap::new();
    params.insert("__script_name", "stop");

    client
        .post(monitor_url)
        .form(&params)
        .send()
        .await
        .map(|resp| tracing::debug!("{resp:?}"))?;

    Ok(())
}
