use crate::{batch_pool::generators, SmallRng};
use anyhow::{anyhow, Result};
use dyn_clonable::*;
use futures::Future;
use futures_timer::Delay;
use gclient::WSAddress;
use rand::{Rng, RngCore, SeedableRng};
use reqwest::Client;
use std::{
    collections::HashMap,
    fs::File,
    io::Write,
    iter,
    ops::Deref,
    result::Result as StdResult,
    time::{Duration, SystemTime, UNIX_EPOCH},
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

pub fn now() -> u64 {
    let time_since_epoch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Internal error: current time before UNIX Epoch");

    time_since_epoch.as_millis() as u64
}

pub fn dump_with_seed(seed: u64) -> Result<()> {
    let code = generators::generate_gear_program::<SmallRng>(seed);

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

pub fn convert_iter<V, T: Into<V>>(args: Vec<T>) -> impl IntoIterator<Item = V> {
    args.into_iter().map(Into::into)
}

#[clonable]
pub trait LoaderRngCore: RngCore + Clone {}
impl<T: RngCore + Clone> LoaderRngCore for T {}

pub trait LoaderRng: Rng + SeedableRng + 'static + Clone {}
impl<T: Rng + SeedableRng + 'static + Clone> LoaderRng for T {}

#[derive(Debug, Clone)]
pub struct NonEmptyVec<T>(Vec<T>);

impl<T> NonEmptyVec<T> {
    pub fn try_from_iter<I>(other: I) -> Result<Self, ()>
    where
        I: Iterator<Item = T>,
    {
        let mut peekable = other.peekable();
        (peekable.peek().is_some())
            .then_some(Self(peekable.collect()))
            .ok_or(())
    }

    pub fn ring_get(&self, index: usize) -> &T {
        assert!(!self.is_empty(), "NonEmptyVec instance is empty");
        &self[index % self.len()]
    }
}

impl<T> Deref for NonEmptyVec<T> {
    type Target = [T];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
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
